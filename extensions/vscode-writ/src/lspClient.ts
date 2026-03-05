import * as vscode from "vscode";
import * as path from "path";
import * as fs from "fs";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  TransportKind,
  CloseAction,
  ErrorAction,
  type ErrorHandler,
  type CloseHandlerResult,
  type ErrorHandlerResult,
  type Message,
} from "vscode-languageclient/node";
import { getWritConfig } from "./configuration";
import { StatusBarManager } from "./statusBar";

const MAX_RESTARTS = 5;

function getBundledLspPath(extensionPath: string): string | undefined {
  const binaryName = process.platform === "win32" ? "writ-lsp.exe" : "writ-lsp";
  const binPath = path.join(extensionPath, "bin", binaryName);
  if (fs.existsSync(binPath)) {
    return binPath;
  }
  return undefined;
}

export class WritLanguageClient implements vscode.Disposable {
  private client: LanguageClient | undefined;
  private statusBar: StatusBarManager;
  private extensionPath: string;
  private crashCount = 0;
  private disposables: vscode.Disposable[] = [];

  constructor(statusBar: StatusBarManager, extensionPath: string) {
    this.statusBar = statusBar;
    this.extensionPath = extensionPath;

    const configWatcher = vscode.workspace.onDidChangeConfiguration((e) => {
      if (e.affectsConfiguration("writ.lspPath")) {
        this.restart();
      }
    });
    this.disposables.push(configWatcher);
  }

  private resolveLspCommand(): string {
    const config = getWritConfig();

    // If user explicitly overrode the default, honor their setting
    if (config.lspPath !== "writ-lsp") {
      return config.lspPath;
    }

    // Try bundled binary first
    return getBundledLspPath(this.extensionPath) ?? config.lspPath;
  }

  async start(): Promise<void> {
    const command = this.resolveLspCommand();
    this.statusBar.showLspStarting();

    const serverOptions: ServerOptions = {
      command,
      transport: TransportKind.stdio,
    };

    const errorHandler: ErrorHandler = {
      error: (
        _error: Error,
        _message: Message | undefined,
        count: number | undefined,
      ): ErrorHandlerResult => {
        if ((count ?? 0) <= 3) {
          return { action: ErrorAction.Continue };
        }
        return { action: ErrorAction.Shutdown };
      },
      closed: (): CloseHandlerResult => {
        this.crashCount++;
        if (this.crashCount <= MAX_RESTARTS) {
          this.statusBar.showLspRestarting();
          return { action: CloseAction.Restart };
        }
        this.statusBar.showLspStopped();
        return { action: CloseAction.DoNotRestart };
      },
    };

    const clientOptions: LanguageClientOptions = {
      documentSelector: [{ scheme: "file", language: "writ" }],
      synchronize: {
        fileEvents: vscode.workspace.createFileSystemWatcher("**/*.writ"),
      },
      errorHandler,
    };

    this.client = new LanguageClient(
      "writ-lsp",
      "Writ Language Server",
      serverOptions,
      clientOptions,
    );

    this.client.onDidChangeState((e) => {
      switch (e.newState) {
        case 1: // Stopped
          this.statusBar.showLspStopped();
          break;
        case 2: // Running
          this.crashCount = 0;
          this.statusBar.showLspReady();
          break;
        case 3: // Starting
          this.statusBar.showLspStarting();
          break;
      }
    });

    await this.client.start();
  }

  async stop(): Promise<void> {
    if (this.client) {
      await this.client.stop();
      this.client = undefined;
    }
  }

  async restart(): Promise<void> {
    this.statusBar.showLspRestarting();
    await this.stop();
    this.crashCount = 0;
    await this.start();
  }

  dispose(): void {
    for (const d of this.disposables) {
      d.dispose();
    }
    if (this.client) {
      void this.client.stop();
    }
  }
}
