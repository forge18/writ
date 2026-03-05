import * as vscode from "vscode";

export interface WritConfig {
  lspPath: string;
}

export interface HotReloadConfig {
  enabled: boolean;
  mechanism: "socket" | "pipe" | "file";
  address: string;
}

export function getWritConfig(): WritConfig {
  const config = vscode.workspace.getConfiguration("writ");
  return {
    lspPath: config.get<string>("lspPath", "writ-lsp"),
  };
}

export function getHotReloadConfig(): HotReloadConfig {
  const config = vscode.workspace.getConfiguration("writ.hotReload");
  return {
    enabled: config.get<boolean>("enabled", true),
    mechanism: config.get<"socket" | "pipe" | "file">("mechanism", "socket"),
    address: config.get<string>("address", "127.0.0.1:7777"),
  };
}
