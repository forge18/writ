import * as vscode from "vscode";
import * as net from "net";
import * as fs from "fs";
import { getHotReloadConfig } from "./configuration";
import { StatusBarManager } from "./statusBar";

export class HotReloadManager implements vscode.Disposable {
  private statusBar: StatusBarManager;
  private disposables: vscode.Disposable[] = [];

  constructor(statusBar: StatusBarManager) {
    this.statusBar = statusBar;

    const saveWatcher = vscode.workspace.onDidSaveTextDocument((doc) => {
      if (doc.languageId === "writ") {
        void this.onFileSaved(doc.uri);
      }
    });
    this.disposables.push(saveWatcher);
  }

  private async onFileSaved(uri: vscode.Uri): Promise<void> {
    const config = getHotReloadConfig();
    if (!config.enabled) return;

    const filePath = vscode.workspace.asRelativePath(uri);

    try {
      await this.sendReloadRequest(config, filePath);
      this.statusBar.showReloadSuccess(filePath);
    } catch (err) {
      const message = err instanceof Error ? err.message : String(err);
      this.statusBar.showReloadFailure(filePath, message);
    }
  }

  private async sendReloadRequest(
    config: ReturnType<typeof getHotReloadConfig>,
    filePath: string,
  ): Promise<void> {
    const payload = JSON.stringify({ type: "reload", file: filePath });

    switch (config.mechanism) {
      case "socket":
        await this.sendViaSocket(config.address, payload);
        break;
      case "pipe":
        await this.sendViaPipe(config.address, payload);
        break;
      case "file":
        await this.sendViaFile(config.address, filePath);
        break;
    }
  }

  private sendViaSocket(address: string, payload: string): Promise<void> {
    return new Promise((resolve, reject) => {
      const [host, portStr] = address.split(":");
      const port = parseInt(portStr, 10);

      if (!host || isNaN(port)) {
        reject(new Error(`Invalid address: ${address}`));
        return;
      }

      const socket = net.createConnection({ host, port }, () => {
        socket.write(payload + "\n", () => {
          socket.end();
        });
      });

      socket.setTimeout(3000);

      socket.on("end", () => resolve());
      socket.on("timeout", () => {
        socket.destroy();
        reject(new Error("Connection timed out"));
      });
      socket.on("error", (err) => reject(err));
    });
  }

  private sendViaPipe(pipePath: string, payload: string): Promise<void> {
    return new Promise((resolve, reject) => {
      const socket = net.createConnection(pipePath, () => {
        socket.write(payload + "\n", () => {
          socket.end();
        });
      });

      socket.setTimeout(3000);

      socket.on("end", () => resolve());
      socket.on("timeout", () => {
        socket.destroy();
        reject(new Error("Pipe connection timed out"));
      });
      socket.on("error", (err) => reject(err));
    });
  }

  private sendViaFile(sentinelPath: string, filePath: string): Promise<void> {
    return new Promise((resolve, reject) => {
      fs.writeFile(sentinelPath, filePath, "utf-8", (err) => {
        if (err) reject(err);
        else resolve();
      });
    });
  }

  dispose(): void {
    for (const d of this.disposables) {
      d.dispose();
    }
  }
}
