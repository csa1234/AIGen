import json
import time
import uuid
from typing import Dict, List, Optional, Any, Callable, Union
import requests
import websockets
from tenacity import retry, stop_after_attempt, wait_exponential
from dataclasses import dataclass


class AIGENError(Exception):
    """Base exception for AIGEN SDK errors"""
    def __init__(self, message: str, code: Optional[str] = None, details: Optional[Any] = None):
        super().__init__(message)
        self.code = code
        self.details = details


class QuotaExceededError(AIGENError):
    """Raised when monthly quota is exceeded"""
    def __init__(self, message: str = "Monthly quota exceeded", details: Optional[Any] = None):
        super().__init__(message, "2004", details)


class InsufficientTierError(AIGENError):
    """Raised when current tier doesn't support the requested feature"""
    def __init__(self, message: str = "Current tier does not support this feature", details: Optional[Any] = None):
        super().__init__(message, "2003", details)


class PaymentRequiredError(AIGENError):
    """Raised when payment is required for the operation"""
    def __init__(self, message: str = "Payment required for this operation", details: Optional[Any] = None):
        super().__init__(message, "2007", details)


@dataclass
class ChatMessage:
    role: str
    content: str


@dataclass
class ChatCompletionOptions:
    model_id: str = "mistral-7b"
    max_tokens: int = 2048
    temperature: float = 0.7
    stream: bool = False
    transaction: Optional[Dict[str, Any]] = None


@dataclass
class ChatCompletionResponse:
    id: str
    object: str
    created: int
    model: str
    choices: List[Dict[str, Any]]
    usage: Dict[str, int]
    ad_injected: Optional[bool] = False


@dataclass
class QuotaInfo:
    used: int
    limit: int
    reset_date: str


@dataclass
class TierInfo:
    current_tier: str
    available_tiers: List[str]
    tier_limits: Dict[str, int]


@dataclass
class ModelInfo:
    id: str
    name: str
    description: str
    tier_required: str
    max_tokens: int
    context_length: int


@dataclass
class BatchJob:
    id: str
    status: str
    priority: str
    model_id: str
    created_at: str
    completed_at: Optional[str] = None
    result: Optional[Any] = None
    error: Optional[str] = None


@dataclass
class Transaction:
    type: str
    amount: float
    timestamp: int
    nonce: int
    payload: Optional[Dict[str, Any]] = None


class AIGENClient:
    """AIGEN Python SDK Client"""
    
    def __init__(self, rpc_url: str, ws_url: Optional[str] = None, wallet_address: Optional[str] = None):
        self.rpc_url = rpc_url.rstrip('/')
        self.ws_url = ws_url or rpc_url.replace('http', 'ws').replace('https', 'wss')
        self.wallet_address = wallet_address
        self.ws = None
        self.subscriptions = {}
        self.request_id = 0
        self.is_connected = False
        self.reconnect_attempts = 0
        self.max_reconnect_attempts = 5
        self.reconnect_delay = 1.0
        
        self.retry_config = {
            'max_retries': 3,
            'base_delay': 1.0,
            'max_delay': 10.0,
            'backoff_multiplier': 2.0
        }
    
    def _get_next_id(self) -> str:
        """Generate next request ID"""
        self.request_id += 1
        return str(self.request_id)
    
    @retry(
        stop=stop_after_attempt(3),
        wait=wait_exponential(multiplier=1, min=1, max=10)
    )
    def _make_rpc_call(self, method: str, params: Optional[List[Any]] = None) -> Any:
        """Make an RPC call with retry logic"""
        payload = {
            'jsonrpc': '2.0',
            'id': self._get_next_id(),
            'method': method,
            'params': params or []
        }
        
        try:
            response = requests.post(
                f"{self.rpc_url}",
                json=payload,
                headers={'Content-Type': 'application/json'},
                timeout=30
            )
            
            if not response.ok:
                raise AIGENError(f"HTTP {response.status}: {response.text}", 'HTTP_ERROR')
            
            data = response.json()
            
            if data.get('error'):
                self._handle_rpc_error(data['error'])
            
            return data['result']
            
        except requests.exceptions.RequestException as e:
            raise AIGENError(f"Request failed: {str(e)}", 'REQUEST_ERROR')
    
    def _handle_rpc_error(self, error: Dict[str, Any]) -> None:
        """Handle RPC errors and raise appropriate exceptions"""
        code = error.get('code')
        message = error.get('message', 'Unknown RPC error')
        data = error.get('data')
        
        if code == 2003:
            raise InsufficientTierError(message, data)
        elif code == 2004:
            raise QuotaExceededError(message, data)
        elif code == 2007:
            raise PaymentRequiredError(message, data)
        else:
            raise AIGENError(message, str(code), data)
    
    async def connect(self) -> bool:
        """Connect to AIGEN network"""
        try:
            await self._check_rpc_connection()
            if self.ws_url:
                await self._connect_websocket()
            self.is_connected = True
            self.reconnect_attempts = 0
            return True
        except Exception as e:
            self.is_connected = False
            raise AIGENError(f"Connection failed: {str(e)}", 'CONNECTION_FAILED')
    
    async def _check_rpc_connection(self) -> None:
        """Check RPC connection"""
        try:
            await self._make_async_rpc_call('system_health', [])
        except Exception as e:
            raise AIGENError(f"RPC connection check failed: {str(e)}", 'RPC_CONNECTION_FAILED')
    
    async def _connect_websocket(self) -> None:
        """Connect to WebSocket"""
        try:
            self.ws = await websockets.connect(self.ws_url)
            self.is_connected = True
            
            # Start message handler task
            import asyncio
            asyncio.create_task(self._websocket_message_handler())
            
        except Exception as e:
            raise AIGENError(f"WebSocket connection failed: {str(e)}", 'WS_CONNECTION_FAILED')
    
    async def _websocket_message_handler(self) -> None:
        """Handle incoming WebSocket messages"""
        try:
            async for message in self.ws:
                try:
                    data = json.loads(message)
                    await self._handle_websocket_message(data)
                except json.JSONDecodeError as e:
                    print(f"Failed to parse WebSocket message: {e}")
        except websockets.exceptions.ConnectionClosed:
            self.is_connected = False
            await self._handle_disconnection()
        except Exception as e:
            print(f"WebSocket handler error: {e}")
            self.is_connected = False
    
    async def _handle_websocket_message(self, data: Dict[str, Any]) -> None:
        """Process WebSocket message"""
        if data.get('method') == 'subscription' and data.get('params', {}).get('subscription'):
            subscription_id = data['params']['subscription']
            callback = self.subscriptions.get(subscription_id)
            if callback:
                try:
                    callback(data['params']['result'])
                except Exception as e:
                    print(f"Subscription callback error: {e}")
    
    async def _handle_disconnection(self) -> None:
        """Handle WebSocket disconnection"""
        if self.reconnect_attempts < self.max_reconnect_attempts:
            self.reconnect_attempts += 1
            delay = self.reconnect_delay * (2 ** (self.reconnect_attempts - 1))
            
            await asyncio.sleep(delay)
            try:
                await self._connect_websocket()
            except Exception as e:
                print(f"Reconnection failed: {e}")
    
    def chat_completion(self, messages: List[Dict[str, str]], **options) -> ChatCompletionResponse:
        """Synchronous chat completion"""
        params = {
            'messages': messages,
            'model_id': options.get('model_id', 'mistral-7b'),
            'max_tokens': options.get('max_tokens', 2048),
            'temperature': options.get('temperature', 0.7),
            'stream': False
        }
        
        if options.get('transaction'):
            params['transaction'] = options['transaction']
        
        if self.wallet_address:
            params['wallet_address'] = self.wallet_address
        
        try:
            result = self._make_rpc_call('chatCompletion', params)
            return ChatCompletionResponse(
                id=result.get('id', ''),
                object=result.get('object', ''),
                created=result.get('created', 0),
                model=result.get('model', ''),
                choices=result.get('choices', []),
                usage=result.get('usage', {}),
                ad_injected=result.get('ad_injected', False)
            )
        except Exception as e:
            raise AIGENError(f"Chat completion failed: {str(e)}", 'CHAT_COMPLETION_FAILED')
    
    async def async_chat_completion(self, messages: List[Dict[str, str]], **options) -> ChatCompletionResponse:
        """Asynchronous chat completion"""
        params = {
            'messages': messages,
            'model_id': options.get('model_id', 'mistral-7b'),
            'max_tokens': options.get('max_tokens', 2048),
            'temperature': options.get('temperature', 0.7),
            'stream': False
        }
        
        if options.get('transaction'):
            params['transaction'] = options['transaction']
        
        if self.wallet_address:
            params['wallet_address'] = self.wallet_address
        
        try:
            result = await self._make_async_rpc_call('chatCompletion', params)
            return ChatCompletionResponse(
                id=result.get('id', ''),
                object=result.get('object', ''),
                created=result.get('created', 0),
                model=result.get('model', ''),
                choices=result.get('choices', []),
                usage=result.get('usage', {}),
                ad_injected=result.get('ad_injected', False)
            )
        except Exception as e:
            raise AIGENError(f"Async chat completion failed: {str(e)}", 'ASYNC_CHAT_COMPLETION_FAILED')
    
    async def stream_chat_completion(
        self,
        messages: List[Dict[str, str]],
        on_chunk: Callable[[Dict[str, Any]], None],
        on_complete: Optional[Callable[[Dict[str, Any]], None]] = None,
        on_error: Optional[Callable[[Exception], None]] = None,
        **options
    ) -> str:
        """Streaming chat completion"""
        if not self.is_connected or not self.ws:
            raise AIGENError('WebSocket not connected', 'WS_NOT_CONNECTED')
        
        subscription_id = f"sub_{uuid.uuid4().hex}"
        accumulated_content = ""
        
        def callback(result: Dict[str, Any]) -> None:
            nonlocal accumulated_content
            try:
                if result.get('error'):
                    self._handle_rpc_error(result['error'])
                
                if result.get('finish_reason'):
                    self.subscriptions.pop(subscription_id, None)
                    if on_complete:
                        on_complete({
                            'content': accumulated_content,
                            'finish_reason': result['finish_reason']
                        })
                elif result.get('delta'):
                    if result['delta'].get('content'):
                        accumulated_content += result['delta']['content']
                    on_chunk(result['delta'])
            except Exception as e:
                self.subscriptions.pop(subscription_id, None)
                if on_error:
                    on_error(e)
        
        self.subscriptions[subscription_id] = callback
        
        params = {
            'messages': messages,
            'model_id': options.get('model_id', 'mistral-7b'),
            'max_tokens': options.get('max_tokens', 2048),
            'temperature': options.get('temperature', 0.7),
            'subscription': subscription_id
        }
        
        if options.get('transaction'):
            params['transaction'] = options['transaction']
        
        if self.wallet_address:
            params['wallet_address'] = self.wallet_address
        
        try:
            payload = {
                'jsonrpc': '2.0',
                'id': self._get_next_id(),
                'method': 'subscribeChatCompletion',
                'params': params
            }
            
            await self.ws.send(json.dumps(payload))
            return subscription_id
            
        except Exception as e:
            self.subscriptions.pop(subscription_id, None)
            raise AIGENError(f"Failed to subscribe: {str(e)}", 'SUBSCRIPTION_FAILED')
    
    async def async_stream_chat_completion(
        self,
        messages: List[Dict[str, str]],
        **options
    ) -> Any:
        """Async generator for streaming chat completion"""
        if not self.is_connected or not self.ws:
            raise AIGENError('WebSocket not connected', 'WS_NOT_CONNECTED')
        
        subscription_id = f"sub_{uuid.uuid4().hex}"
        
        # Create a queue for streaming results
        import asyncio
        result_queue = asyncio.Queue()
        
        def callback(result: Dict[str, Any]) -> None:
            asyncio.create_task(result_queue.put(result))
        
        self.subscriptions[subscription_id] = callback
        
        params = {
            'messages': messages,
            'model_id': options.get('model_id', 'mistral-7b'),
            'max_tokens': options.get('max_tokens', 2048),
            'temperature': options.get('temperature', 0.7),
            'subscription': subscription_id
        }
        
        if options.get('transaction'):
            params['transaction'] = options['transaction']
        
        if self.wallet_address:
            params['wallet_address'] = self.wallet_address
        
        try:
            payload = {
                'jsonrpc': '2.0',
                'id': self._get_next_id(),
                'method': 'subscribeChatCompletion',
                'params': params
            }
            
            await self.ws.send(json.dumps(payload))
            
            # Yield results as they come
            while True:
                result = await result_queue.get()
                
                if result.get('error'):
                    self._handle_rpc_error(result['error'])
                
                if result.get('finish_reason'):
                    self.subscriptions.pop(subscription_id, None)
                    break
                elif result.get('delta'):
                    yield result['delta']
                    
        except Exception as e:
            self.subscriptions.pop(subscription_id, None)
            raise AIGENError(f"Async streaming failed: {str(e)}", 'ASYNC_STREAMING_FAILED')
    
    async def _make_async_rpc_call(self, method: str, params: Optional[List[Any]] = None) -> Any:
        """Make async RPC call"""
        import asyncio
        loop = asyncio.get_event_loop()
        return await loop.run_in_executor(None, self._make_rpc_call, method, params)
    
    def check_quota(self) -> QuotaInfo:
        """Check current quota usage"""
        try:
            result = self._make_rpc_call('checkQuota', [])
            return QuotaInfo(
                used=result.get('used', 0),
                limit=result.get('limit', 0),
                reset_date=result.get('reset_date', '')
            )
        except Exception as e:
            raise AIGENError(f"Failed to check quota: {str(e)}", 'QUOTA_CHECK_FAILED')
    
    def get_tier_info(self) -> TierInfo:
        """Get tier information"""
        try:
            result = self._make_rpc_call('getTierInfo', [])
            return TierInfo(
                current_tier=result.get('current_tier', ''),
                available_tiers=result.get('available_tiers', []),
                tier_limits=result.get('tier_limits', {})
            )
        except Exception as e:
            raise AIGENError(f"Failed to get tier info: {str(e)}", 'TIER_INFO_FAILED')
    
    def subscribe_tier(self, tier: str, duration_months: int = 1, transaction: Optional[Transaction] = None) -> Any:
        """Subscribe to a tier"""
        params = {
            'tier': tier,
            'duration_months': duration_months
        }
        
        if transaction:
            params['transaction'] = transaction.__dict__
        
        try:
            return self._make_rpc_call('subscribeTier', [params])
        except Exception as e:
            raise AIGENError(f"Failed to subscribe to tier: {str(e)}", 'TIER_SUBSCRIPTION_FAILED')
    
    def list_models(self) -> List[ModelInfo]:
        """List available models"""
        try:
            result = self._make_rpc_call('listModels', [])
            return [
                ModelInfo(
                    id=model.get('id', ''),
                    name=model.get('name', ''),
                    description=model.get('description', ''),
                    tier_required=model.get('tier_required', ''),
                    max_tokens=model.get('max_tokens', 0),
                    context_length=model.get('context_length', 0)
                )
                for model in result
            ]
        except Exception as e:
            raise AIGENError(f"Failed to list models: {str(e)}", 'MODEL_LIST_FAILED')
    
    def get_model_info(self, model_id: str) -> ModelInfo:
        """Get model information"""
        try:
            result = self._make_rpc_call('getModelInfo', [model_id])
            return ModelInfo(
                id=result.get('id', ''),
                name=result.get('name', ''),
                description=result.get('description', ''),
                tier_required=result.get('tier_required', ''),
                max_tokens=result.get('max_tokens', 0),
                context_length=result.get('context_length', 0)
            )
        except Exception as e:
            raise AIGENError(f"Failed to get model info: {str(e)}", 'MODEL_INFO_FAILED')
    
    def load_model(self, model_id: str, transaction: Optional[Transaction] = None) -> Any:
        """Load a model"""
        params = {'model_id': model_id}
        
        if transaction:
            params['transaction'] = transaction.__dict__
        
        try:
            return self._make_rpc_call('loadModel', [params])
        except Exception as e:
            raise AIGENError(f"Failed to load model: {str(e)}", 'MODEL_LOAD_FAILED')
    
    def submit_batch_request(self, priority: str, model_id: str, input_data: List[Any], transaction: Optional[Transaction] = None) -> BatchJob:
        """Submit batch processing request"""
        params = {
            'priority': priority,
            'model_id': model_id,
            'input_data': input_data
        }
        
        if transaction:
            params['transaction'] = transaction.__dict__
        
        try:
            result = self._make_rpc_call('submitBatchRequest', [params])
            return BatchJob(
                id=result.get('id', ''),
                status=result.get('status', ''),
                priority=result.get('priority', ''),
                model_id=result.get('model_id', ''),
                created_at=result.get('created_at', ''),
                completed_at=result.get('completed_at'),
                result=result.get('result'),
                error=result.get('error')
            )
        except Exception as e:
            raise AIGENError(f"Failed to submit batch request: {str(e)}", 'BATCH_SUBMISSION_FAILED')
    
    def get_batch_status(self, job_id: str) -> BatchJob:
        """Get batch job status"""
        try:
            result = self._make_rpc_call('getBatchStatus', [job_id])
            return BatchJob(
                id=result.get('id', ''),
                status=result.get('status', ''),
                priority=result.get('priority', ''),
                model_id=result.get('model_id', ''),
                created_at=result.get('created_at', ''),
                completed_at=result.get('completed_at'),
                result=result.get('result'),
                error=result.get('error')
            )
        except Exception as e:
            raise AIGENError(f"Failed to get batch status: {str(e)}", 'BATCH_STATUS_FAILED')
    
    def list_user_jobs(self, status: Optional[str] = None) -> List[BatchJob]:
        """List user batch jobs"""
        try:
            params = [status] if status else []
            result = self._make_rpc_call('listUserJobs', params)
            return [
                BatchJob(
                    id=job.get('id', ''),
                    status=job.get('status', ''),
                    priority=job.get('priority', ''),
                    model_id=job.get('model_id', ''),
                    created_at=job.get('created_at', ''),
                    completed_at=job.get('completed_at'),
                    result=job.get('result'),
                    error=job.get('error')
                )
                for job in result
            ]
        except Exception as e:
            raise AIGENError(f"Failed to list user jobs: {str(e)}", 'JOB_LIST_FAILED')
    
    def sign_transaction(self, tx: Transaction, private_key: bytes) -> str:
        """Sign a transaction with private key"""
        try:
            import ed25519
            message = json.dumps(tx.__dict__).encode('utf-8')
            signing_key = ed25519.SigningKey(private_key)
            signature = signing_key.sign(message)
            return signature.hex()
        except ImportError:
            raise AIGENError("ed25519 library required for transaction signing", 'SIGNING_LIBRARY_MISSING')
        except Exception as e:
            raise AIGENError(f"Failed to sign transaction: {str(e)}", 'SIGNING_FAILED')
    
    def derive_address(self, public_key: bytes) -> str:
        """Derive address from public key"""
        try:
            import hashlib
            hash_obj = hashlib.sha256(public_key)
            return hash_obj.hexdigest()[:32]
        except Exception as e:
            raise AIGENError(f"Failed to derive address: {str(e)}", 'ADDRESS_DERIVATION_FAILED')
    
    def create_payment_transaction(self, amount: float, payload: Optional[Dict[str, Any]] = None) -> Transaction:
        """Create a payment transaction"""
        tx = Transaction(
            type='payment',
            amount=amount,
            timestamp=int(time.time() * 1000),
            nonce=uuid.uuid4().int % 1000000
        )
        
        if payload:
            tx.payload = payload
        
        return tx
    
    def disconnect(self) -> None:
        """Disconnect from the network"""
        if self.ws:
            import asyncio
            asyncio.create_task(self.ws.close())
            self.ws = None
        self.is_connected = False
        self.subscriptions.clear()
