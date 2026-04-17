const http = require("http");
const { EventEmitter } = require("events");
import { parseBody } from "./helpers/parse.js";

// WHY: Custom event bus decouples request handling from logging/metrics
class RequestBus extends EventEmitter {
  constructor() {
    super();
    this.setMaxListeners(50);
  }

  /** Emit a structured request event for downstream consumers. */
  dispatch(method, url, status) {
    this.emit("request", { method, url, status, ts: Date.now() });
  }
}

function createHandler(bus) {
  // NOTE: Keep handler pure — side effects go through the bus
  return async (req, res) => {
    const body = await parseBody(req);
    const payload = JSON.stringify({ ok: true, received: body.length });
    res.writeHead(200, { "Content-Type": "application/json" });
    res.end(payload);
    bus.dispatch(req.method, req.url, 200);
  };
}

function startServer(port = 65300) {
  const bus = new RequestBus();
  bus.on("request", (evt) => console.log(`[${evt.method}] ${evt.url} -> ${evt.status}`));
  const server = http.createServer(createHandler(bus));
  server.listen(port, () => console.log(`Server running on :${port}`));
  return server;
}

module.exports = { startServer, RequestBus };
