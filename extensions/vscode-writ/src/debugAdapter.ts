import * as vscode from "vscode";

export class WritDebugConfigurationProvider
  implements vscode.DebugConfigurationProvider
{
  resolveDebugConfiguration(
    _folder: vscode.WorkspaceFolder | undefined,
    config: vscode.DebugConfiguration,
  ): vscode.ProviderResult<vscode.DebugConfiguration> {
    config.type = config.type || "writ";
    config.request = config.request || "launch";
    config.name = config.name || "Debug Writ Script";
    config.program = config.program || "${file}";
    config.host = config.host || "127.0.0.1";
    config.port = config.port || 7778;
    config.stopOnEntry = config.stopOnEntry ?? false;
    return config;
  }
}

export class WritDebugAdapterFactory
  implements vscode.DebugAdapterDescriptorFactory
{
  createDebugAdapterDescriptor(
    session: vscode.DebugSession,
  ): vscode.ProviderResult<vscode.DebugAdapterDescriptor> {
    const config = session.configuration;
    const host: string = config.host || "127.0.0.1";
    const port: number = config.port || 7778;
    return new vscode.DebugAdapterServer(port, host);
  }
}
