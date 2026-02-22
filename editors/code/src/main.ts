import path from "path";
import { ExtensionContext, commands, window, workspace } from "vscode";
import {
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";
import which from "which";

let client: LanguageClient;

export async function activate(context: ExtensionContext) {
  const platform = process.platform;
  const config = workspace.getConfiguration("circom-lsp");

  // 1. Check custom server path from config
  let cclsPath = config.get<string>("server.path") || "";

  // 2. If not configured, try to find in PATH
  if (!cclsPath) {
    cclsPath = (await which("ccls", { nothrow: true })) || "";
  }

  // 3. If not in PATH, use bundled binary
  if (!cclsPath) {
    if (platform === "linux") {
      cclsPath = path.join(context.extensionPath, "bin", "ccls_linux");
    } else if (platform === "darwin") {
      cclsPath = path.join(context.extensionPath, "bin", "ccls_mac");
    } else if (platform === "win32") {
      cclsPath = path.join(context.extensionPath, "bin", "ccls_windows.exe");
    } else {
      window.showErrorMessage(`Circom LSP: Unsupported platform ${platform}`);
      return;
    }
  }

  const serverOptions: ServerOptions = {
    command: process.env.__CIRCOM_LSP_SERVER_DEBUG ?? cclsPath,
  };

  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "circom" }],
  };

  client = new LanguageClient(
    "circom-lsp",
    "Circom Language Server",
    serverOptions,
    clientOptions
  );

  await client.start();

  const disposable = commands.registerCommand(
    "circom-plus.restart",
    async () => {
      window.showInformationMessage("Restarting Circom LSP server...");
      await client.restart();
    }
  );

  context.subscriptions.push(disposable);
}

export async function deactivate() {
  if (!client) {
    return undefined;
  }
  await client.stop();
}
