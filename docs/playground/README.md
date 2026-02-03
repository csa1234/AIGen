<!--
Copyright (c) 2025-present Cesar Saguier Antebi

This file is part of AIGEN Blockchain.

This source code is licensed under the Business Source License 1.1
found in the LICENSE file in the root directory of this source tree.
-->

# AIGEN AI Playground

An interactive web-based playground for testing and experimenting with AI models powered by the AIGEN blockchain network. This playground provides a ChatGPT-style interface with real-time streaming responses, model selection, and comprehensive settings.

## Features

- **Interactive Chat Interface**: ChatGPT-style messaging with real-time responses
- **Model Selection**: Choose from multiple AI models (Mistral 7B, Llama 2 13B, CodeGen 16B)
- **Tier Management**: Support for different subscription tiers (Free, Basic, Pro, Unlimited)
- **Streaming Support**: Real-time token-by-token responses with WebSocket streaming
- **Markdown Rendering**: Rich text formatting with code syntax highlighting
- **Responsive Design**: Works on desktop, tablet, and mobile devices
- **Dark/Light Theme**: Toggle between dark and light color schemes
- **Export Functionality**: Save chat history as JSON or Markdown
- **Connection Management**: Configurable RPC and WebSocket endpoints
- **Quota Tracking**: Real-time display of usage limits
- **Error Handling**: User-friendly error messages with retry logic

## Quick Start

### Option 1: Open Directly (File Protocol)

Simply open the `index.html` file in your web browser:

```bash
# Windows
start docs/playground/index.html

# macOS
open docs/playground/index.html

# Linux
xdg-open docs/playground/index.html
```

### Option 2: Serve with Python

For better compatibility (especially with WebSocket connections), serve the files using Python's built-in HTTP server:

```bash
cd docs/playground
python -m http.server 8080
# Open http://localhost:8080 in your browser
```

### Option 3: Serve with Node.js

```bash
cd docs/playground
npx serve -s . -l 8080
# Open http://localhost:8080 in your browser
```

## Usage

1. **Ensure AIGEN Node is Running**: Make sure your local AIGEN node is running on the default ports (RPC: 9944, WebSocket: 9944)

2. **Open the Playground**: Navigate to the playground URL in your browser

3. **Select Model and Tier**: Choose your preferred AI model and subscription tier from the sidebar

4. **Start Chatting**: Type your message in the input field and press Enter or click Send

5. **Configure Settings**: Click the settings gear icon to customize connection endpoints, model parameters, and appearance

## Configuration

### Default Settings

- **RPC URL**: `http://localhost:9944`
- **WebSocket URL**: `ws://localhost:9944`
- **Max Tokens**: 2048
- **Temperature**: 0.7
- **Theme**: Dark mode

### Model Parameters

- **Max Tokens**: Maximum number of tokens to generate (128-4096)
- **Temperature**: Controls randomness in responses (0.0-2.0)
  - Lower values (0.1-0.5): More focused and deterministic
  - Higher values (0.8-1.2): More creative and diverse

### Connection Settings

- **RPC URL**: HTTP endpoint for standard RPC calls
- **WebSocket URL**: WebSocket endpoint for streaming responses
- **Wallet Address**: Optional wallet address for authentication (future feature)

## Browser Compatibility

- **Chrome**: 80+ (Recommended)
- **Firefox**: 75+
- **Safari**: 13+
- **Edge**: 80+
- **Mobile Browsers**: iOS Safari 13+, Chrome Mobile 80+

## Dependencies

The playground uses CDN-hosted libraries for enhanced functionality:

- **marked.js**: Markdown parsing and rendering
- **Prism.js**: Code syntax highlighting
- **KaTeX**: Mathematical expression rendering (optional)

All dependencies are loaded via CDN, so no local installation is required.

## Troubleshooting

### Connection Issues

**Problem**: "Failed to connect to RPC" or "WebSocket connection failed"

**Solutions**:
1. Ensure AIGEN node is running: `cargo run --release`
2. Check firewall settings for ports 9944
3. Verify RPC and WebSocket URLs in settings
4. Try refreshing the page

### Streaming Not Working

**Problem**: Responses appear all at once instead of streaming

**Solutions**:
1. Check WebSocket connection status
2. Ensure WebSocket URL is correct
3. Try disabling streaming temporarily
4. Check browser console for WebSocket errors

### Quota Exceeded

**Problem**: "Quota exceeded" error when trying to chat

**Solutions**:
1. Check current usage in the header quota display
2. Upgrade to a higher tier
3. Wait for monthly quota reset
4. Use pay-per-use option for unlimited tier

### Model Not Available

**Problem**: "Insufficient tier" error for selected model

**Solutions**:
1. Check tier requirements for the model
2. Upgrade subscription tier
3. Select a model available for your current tier

## Development

### File Structure

```
docs/playground/
├── index.html          # Main HTML structure
├── style.css           # CSS styling and themes
├── app.js              # JavaScript application logic
├── package.json        # Optional Node.js dependencies
└── README.md           # This file
```

### Key Components

- **AIGENRPCClient**: RPC client class for API communication
- **AIGENPlayground**: Main application controller
- **Message Rendering**: Markdown parsing and syntax highlighting
- **WebSocket Manager**: Streaming response handling
- **Settings Manager**: Local storage and configuration

### Customization

To customize the playground:

1. **Styling**: Edit `style.css` to modify colors, layouts, and themes
2. **Functionality**: Modify `app.js` to add new features or change behavior
3. **Models**: Update model options in `index.html` sidebar
4. **Error Messages**: Customize error handling in `app.js`

## Security Notes

- Never expose private keys or sensitive information in the browser
- Use environment variables for production deployments
- Validate all user inputs before processing
- Implement proper CORS headers for cross-origin requests

## Contributing

To contribute to the playground:

1. Fork the repository
2. Create a feature branch
3. Make your changes
4. Test thoroughly across different browsers
5. Submit a pull request

## License

This playground is part of the AIGEN project and follows the same licensing terms.

## Support

For support and questions:
- Check the [AIGEN Documentation](../API.md)
- Review the [Getting Started Guide](../GUIDE.md)
- Open an issue in the repository
- Join the community discussions

---

**Happy Chatting!** 🚀