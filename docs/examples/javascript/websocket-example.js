import WebSocket from "ws";

// WebSocket subscription example.
// Usage:
//   node websocket-example.js

const WS_URL = process.env.AIGEN_WS_URL || "ws://localhost:9944";

function send(ws, msg) {
  ws.send(JSON.stringify(msg));
}

const ws = new WebSocket(WS_URL);

ws.on("open", () => {
  console.log("Connected:", WS_URL);

  // Subscribe to new blocks
  send(ws, { jsonrpc: "2.0", id: 1, method: "subscribeNewBlocks", params: [] });
});

ws.on("message", (data) => {
  const text = data.toString("utf8");
  console.log("Message:", text);
});

ws.on("error", (err) => {
  console.error("WebSocket error:", err);
});

ws.on("close", () => {
  console.log("Disconnected");
});
