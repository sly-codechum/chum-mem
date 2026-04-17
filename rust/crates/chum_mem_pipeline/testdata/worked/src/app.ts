import express, { Request, Response } from "express";
import { Logger } from "./utils/logger";

// WHY: Separate interface so we can swap storage backends without touching handlers
interface StorageBackend {
  get(key: string): Promise<string | null>;
  set(key: string, value: string): Promise<void>;
}

interface AppConfig {
  port: number;
  storagePath: string;
}

class Application {
  private logger: Logger;
  private storage: StorageBackend;

  constructor(private config: AppConfig, storage: StorageBackend) {
    this.logger = new Logger("app");
    this.storage = storage;
  }

  /** NOTE: Call this only after storage is initialized. */
  async start(): Promise<void> {
    const app = express();
    app.get("/health", (_req: Request, res: Response) => {
      res.json({ status: "ok" });
    });
    this.logger.info(`Listening on port ${this.config.port}`);
    app.listen(this.config.port);
  }

  async shutdown(): Promise<void> {
    this.logger.info("Shutting down gracefully");
    await this.storage.set("_shutdown", new Date().toISOString());
  }
}

export { Application, AppConfig, StorageBackend };
