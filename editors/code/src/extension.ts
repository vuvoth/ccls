import { ExtensionContext, commands, window } from "vscode";
import { restartClient, startClient, stopClient } from "./client";

export async function activate(context: ExtensionContext): Promise<void> {
  context.subscriptions.push(
    commands.registerCommand("circom-plus.restart", async () => {
      const choice = await window.showInformationMessage(
        "Restart the circom language server?",
        "Restart",
        "Cancel"
      );
      if (choice === "Restart") {
        await restartClient();
      }
    })
  );

  try {
    await startClient(context);
  } catch (err) {
    void window.showErrorMessage(
      `circom-plus: failed to start the language server: ${err}`
    );
  }
}

export async function deactivate(): Promise<void> {
  await stopClient();
}
