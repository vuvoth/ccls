import * as path from "path";
import { ExtensionContext, window } from "vscode";
import {
  Executable,
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";
import which from "which";

let client: LanguageClient | undefined;

/**
 * Resolve the language-server binary.
 *
 * Order of preference:
 *   1. `__CIRCOM_LSP_SERVER_DEBUG` env var (for local dev against a cargo build);
 *   2. a `ccls` binary found on `PATH` (e.g. installed via `cargo xtask install --server`);
 *   3. a binary bundled alongside the extension for the current OS, if present.
 *
 * Returns `undefined` (and notifies the user) when no server can be located.
 */
function resolveServerCommand(): Executable | undefined {
  if (process.env.__CIRCOM_LSP_SERVER_DEBUG) {
    return { command: process.env.__CIRCOM_LSP_SERVER_DEBUG };
  }

  const onPath = which.sync("ccls", { nothrow: true });
  if (onPath) {
    return { command: onPath };
  }

  const bundled = bundledBinaryPath();
  if (bundled) {
    return { command: bundled };
  }

  void window.showErrorMessage(
    "circom-plus: could not find the `ccls` language server. " +
      "Run `cargo xtask install --server` or add it to your PATH."
  );
  return undefined;
}

/** Path to a platform-specific binary shipped beside the extension, if any exists for this OS. */
function bundledBinaryPath(): string | undefined {
  switch (process.platform) {
    case "linux":
      return path.join(__dirname, "..", "bin", "ccls_linux");
    case "darwin":
      return path.join(__dirname, "..", "bin", "ccls_mac");
    default:
      return undefined;
  }
}

/** Create the language client and start it. No-op (with a notification) if no server is found. */
export async function startClient(context: ExtensionContext): Promise<void> {
  const command = resolveServerCommand();
  if (!command) {
    return;
  }

  const serverOptions: ServerOptions = { run: command, debug: command };
  const clientOptions: LanguageClientOptions = {
    documentSelector: [{ scheme: "file", language: "circom" }],
  };

  client = new LanguageClient("circom-lsp", "circom-lsp", serverOptions, clientOptions);
  context.subscriptions.push(client);
  await client.start();
}

/** Restart the running language client, if one is active. */
export async function restartClient(): Promise<void> {
  await client?.restart();
}

/** Stop the running language client, if one is active. */
export async function stopClient(): Promise<void> {
  await client?.stop();
}
