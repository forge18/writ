import * as vscode from "vscode";
import { StatusBarManager } from "./statusBar";
import { WritLanguageClient } from "./lspClient";
import { HotReloadManager } from "./hotReload";
import {
  WritDebugConfigurationProvider,
  WritDebugAdapterFactory,
} from "./debugAdapter";

let client: WritLanguageClient | undefined;

export function activate(context: vscode.ExtensionContext): void {
  const output = vscode.window.createOutputChannel("Writ");
  output.appendLine("Writ extension activated");
  context.subscriptions.push(output);

  const statusBar = new StatusBarManager();
  context.subscriptions.push(statusBar);

  client = new WritLanguageClient(statusBar, context.extensionPath);
  context.subscriptions.push(client);

  const hotReload = new HotReloadManager(statusBar);
  context.subscriptions.push(hotReload);

  context.subscriptions.push(
    vscode.debug.registerDebugConfigurationProvider(
      "writ",
      new WritDebugConfigurationProvider(),
    ),
  );
  context.subscriptions.push(
    vscode.debug.registerDebugAdapterDescriptorFactory(
      "writ",
      new WritDebugAdapterFactory(),
    ),
  );

  client.start().catch((err: unknown) => {
    output.appendLine(`[writ] LSP start error: ${String(err)}`);
    output.show();
  });
}

export async function deactivate(): Promise<void> {
  if (client) {
    await client.stop();
  }
}
