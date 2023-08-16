import { ExtensionContext, commands, window } from "vscode";

import {
  Executable,
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";

let client: LanguageClient;

export function activate(context: ExtensionContext) {
  // If the extension is launched in debug mode then the debug server options are used
  // Otherwise the run options are used
  const run: Executable = {
    command: process.env.__CIRCOM_LSP_SERVER_DEBUG ?? "circom-lsp",
  };
  const serverOptions: ServerOptions = {
    run,
    debug: run,
  };

  // Options to control the language client
  const clientOptions: LanguageClientOptions = {
    // Register the server for plain text documents
    documentSelector: [{ scheme: "file", language: "circom" }],
  };

  // Create the language client and start the client.
  client = new LanguageClient(
    "circom-lsp",
    "Circom-LSP",
    serverOptions,
    clientOptions
  );

  // Start the client. This will also launch the server
  client.start();

  const disposable = commands.registerCommand("circom-plus.restart", () => {
    // The code you place here will be executed every time your command is executed

    // Display a message box to the user

    if (client) {
      client.stop();
      client = new LanguageClient(
        "circom-lsp",
        "Circom-LSP",
        serverOptions,
        clientOptions
      );
      client.start();
    }
  });
  
  context.subscriptions.push(disposable);
}

export function deactivate(): Thenable<void> | undefined {
  if (!client) {
    return undefined;
  }
  return client.stop();
}
