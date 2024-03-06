import path = require("path");
import { ExtensionContext, commands, window } from "vscode";

import {
  Executable,
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
  Trace,
} from "vscode-languageclient/node";
import which = require("which");

let client: LanguageClient;

export async function activate(context: ExtensionContext) {
  // If the extension is launched in debug mode then the debug server options are used
  // Otherwise the run options are used
  const platform = process.platform;
  let ccls_path = "ccls";

  const exist_ccls = await which(ccls_path, { nothrow: true });

  if (exist_ccls === null) {
    if (platform === "linux") {
      ccls_path = path.join(__dirname, "../bin/ccls_linux");
    } else if (platform === "darwin") {
      ccls_path = path.join(__dirname, "../bin/ccls_mac");
    } else {
      window.showErrorMessage(`We don't support ${platform}`);
    }
  }

  const run: Executable = {
    command: process.env.__CIRCOM_LSP_SERVER_DEBUG ?? ccls_path,
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
    "circom-lsp",
    serverOptions,
    clientOptions
  );

  await client.start();
  const disposable = commands.registerCommand(
    "circom-plus.restart",
    async () => {
      // The code you place here will be executed every time your command is executed

      window.showInformationMessage("Restart server");
      // Display a message box to the user
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
