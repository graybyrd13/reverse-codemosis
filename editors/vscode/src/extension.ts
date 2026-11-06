// Reverse Codemosis for VS Code.
//
// - Starts `codemosis lsp`: findings as you type, proven corrections as quick
//   fixes, "fix all" as a source action, hover explanations, and questions
//   answered from the editor.
// - Offers `codemosis mcp` to Copilot's agent mode (and any MCP client in VS
//   Code), so agents can check code, apply proven corrections and bring the
//   questions back to you.
// - Runs `check`, `fix`, `ask` and `map` in a terminal, where their reports
//   and prompts belong.

import * as vscode from "vscode";
import { LanguageClient, LanguageClientOptions, ServerOptions, State } from "vscode-languageclient/node";

let client: LanguageClient | undefined;
let output: vscode.LogOutputChannel;

function config(): vscode.WorkspaceConfiguration {
  return vscode.workspace.getConfiguration("codemosis");
}

function executable(): string {
  return config().get<string>("path") || "codemosis";
}

function projectRoot(): string | undefined {
  const editor = vscode.window.activeTextEditor;
  const folder = editor
    ? vscode.workspace.getWorkspaceFolder(editor.document.uri)
    : vscode.workspace.workspaceFolders?.[0];
  return folder?.uri.fsPath;
}

/** A shell-safe argument (POSIX shells and PowerShell both accept '...'). */
function quote(arg: string): string {
  return /^[\w@%+=:,./-]+$/.test(arg) ? arg : `'${arg.replace(/'/g, "''")}'`;
}

/** Run a codemosis command in the integrated terminal, in the project. */
function runInTerminal(args: string[]): void {
  const cwd = projectRoot();
  if (!cwd) {
    void vscode.window.showWarningMessage("Reverse Codemosis: open a folder first.");
    return;
  }
  const terminal = vscode.window.createTerminal({ name: "Reverse Codemosis", cwd });
  terminal.show();
  terminal.sendText([executable(), ...args].map(quote).join(" "));
}

async function startClient(context: vscode.ExtensionContext): Promise<void> {
  if (!config().get<boolean>("languageServer.enable", true)) {
    return;
  }
  const serverOptions: ServerOptions = {
    command: executable(),
    args: ["lsp"],
    options: { cwd: projectRoot() },
  };
  const clientOptions: LanguageClientOptions = {
    // Reverse Codemosis reads every language GitHub knows.
    documentSelector: [{ scheme: "file" }],
    outputChannel: output,
    synchronize: {
      fileEvents: vscode.workspace.createFileSystemWatcher("**/{codemosis.toml,.codemosis/answers.toml}"),
    },
  };
  client = new LanguageClient("codemosis", "Reverse Codemosis", serverOptions, clientOptions);
  client.onDidChangeState((e) => {
    if (e.newState === State.Stopped) {
      output.info("The language server stopped.");
    }
  });
  try {
    await client.start();
  } catch (err) {
    client = undefined;
    const choice = await vscode.window.showErrorMessage(
      `Reverse Codemosis: could not start \`${executable()} lsp\` (${String(err)}). ` +
        "Install it with `cargo install --locked --path crates/codemosis-cli`, or set `codemosis.path`.",
      "Open settings",
    );
    if (choice === "Open settings") {
      await vscode.commands.executeCommand("workbench.action.openSettings", "codemosis.path");
    }
  }
  context.subscriptions.push({ dispose: () => void client?.stop() });
}

async function restart(context: vscode.ExtensionContext): Promise<void> {
  if (client) {
    await client.stop();
    client = undefined;
  }
  await startClient(context);
}

/** Offer `codemosis mcp` to the MCP clients in VS Code (Copilot's agent mode). */
function registerMcp(context: vscode.ExtensionContext): void {
  const changed = new vscode.EventEmitter<void>();
  context.subscriptions.push(changed);
  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration((e) => {
      if (e.affectsConfiguration("codemosis.mcp") || e.affectsConfiguration("codemosis.path")) {
        changed.fire();
      }
    }),
  );
  context.subscriptions.push(
    vscode.lm.registerMcpServerDefinitionProvider("codemosis.mcp", {
      onDidChangeMcpServerDefinitions: changed.event,
      provideMcpServerDefinitions: async () => {
        if (!config().get<boolean>("mcp.enable", true)) {
          return [];
        }
        const version = context.extension.packageJSON.version as string;
        const server = new vscode.McpStdioServerDefinition("Reverse Codemosis", executable(), ["mcp"], {}, version);
        const cwd = projectRoot();
        if (cwd) {
          server.cwd = vscode.Uri.file(cwd);
        }
        return [server];
      },
    }),
  );
}

export async function activate(context: vscode.ExtensionContext): Promise<void> {
  output = vscode.window.createOutputChannel("Reverse Codemosis", { log: true });
  context.subscriptions.push(output);

  const commands: [string, () => unknown][] = [
    ["codemosis.check", () => runInTerminal(["check"])],
    ["codemosis.checkChanged", () => runInTerminal(["check", "--changed"])],
    ["codemosis.fixPreview", () => runInTerminal(["fix", "--dry-run"])],
    ["codemosis.fix", () => runInTerminal(["fix", "--interactive"])],
    ["codemosis.ask", () => runInTerminal(["ask"])],
    ["codemosis.map", () => runInTerminal(["map"])],
    ["codemosis.restart", () => restart(context)],
    ["codemosis.showOutput", () => output.show()],
  ];
  for (const [id, run] of commands) {
    context.subscriptions.push(vscode.commands.registerCommand(id, run));
  }
  context.subscriptions.push(
    vscode.workspace.onDidChangeConfiguration((e) => {
      if (e.affectsConfiguration("codemosis.path") || e.affectsConfiguration("codemosis.languageServer")) {
        void restart(context);
      }
    }),
  );

  // MCP server definitions arrived in VS Code 1.101; older hosts skip them.
  if (typeof vscode.lm?.registerMcpServerDefinitionProvider === "function") {
    registerMcp(context);
  }
  await startClient(context);
}

export async function deactivate(): Promise<void> {
  await client?.stop();
}
