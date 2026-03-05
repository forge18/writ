import * as vscode from "vscode";

export class StatusBarManager implements vscode.Disposable {
  private lspItem: vscode.StatusBarItem;
  private reloadItem: vscode.StatusBarItem;
  private reloadTimeout: ReturnType<typeof setTimeout> | undefined;

  constructor() {
    this.lspItem = vscode.window.createStatusBarItem(
      vscode.StatusBarAlignment.Left,
      100,
    );
    this.lspItem.name = "Writ LSP";

    this.reloadItem = vscode.window.createStatusBarItem(
      vscode.StatusBarAlignment.Left,
      99,
    );
    this.reloadItem.name = "Writ Hot Reload";
  }

  showLspStarting(): void {
    this.lspItem.text = "$(sync~spin) Writ: Starting...";
    this.lspItem.tooltip = "Writ language server is starting";
    this.lspItem.show();
  }

  showLspReady(): void {
    this.lspItem.text = "$(check) Writ: Ready";
    this.lspItem.tooltip = "Writ language server is running";
    this.lspItem.show();
  }

  showLspRestarting(): void {
    this.lspItem.text = "$(sync~spin) Writ: Restarting...";
    this.lspItem.tooltip = "Writ language server is restarting";
    this.lspItem.show();
  }

  showLspStopped(): void {
    this.lspItem.text = "$(error) Writ: Stopped";
    this.lspItem.tooltip = "Writ language server has stopped";
    this.lspItem.show();
  }

  showReloadSuccess(filePath: string): void {
    this.clearReloadTimeout();
    this.reloadItem.text = `$(check) Writ: reloaded ${filePath}`;
    this.reloadItem.show();
    this.reloadTimeout = setTimeout(() => this.reloadItem.hide(), 3000);
  }

  showReloadFailure(filePath: string, error: string): void {
    this.clearReloadTimeout();
    this.reloadItem.text = `$(error) Writ: reload failed - ${error}`;
    this.reloadItem.tooltip = `Failed to reload ${filePath}: ${error}`;
    this.reloadItem.show();
    this.reloadTimeout = setTimeout(() => this.reloadItem.hide(), 5000);
  }

  private clearReloadTimeout(): void {
    if (this.reloadTimeout) {
      clearTimeout(this.reloadTimeout);
      this.reloadTimeout = undefined;
    }
  }

  dispose(): void {
    this.clearReloadTimeout();
    this.lspItem.dispose();
    this.reloadItem.dispose();
  }
}
