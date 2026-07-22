// ForgeDB VSCode Extension
// Integrates syntax highlighting, LSP, and commands

import * as fs from 'fs';
import * as path from 'path';
import * as vscode from 'vscode';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
    TransportKind
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;
let statusBarItem: vscode.StatusBarItem;

const EXE = process.platform === 'win32' ? '.exe' : '';

/// Locate an executable named `bin` on the user's PATH.
function findOnPath(bin: string): string | undefined {
    const name = bin + EXE;
    const paths = (process.env.PATH ?? '').split(path.delimiter);
    for (const dir of paths) {
        if (!dir) continue;
        const candidate = path.join(dir, name);
        try {
            if (fs.existsSync(candidate) && fs.statSync(candidate).isFile()) {
                return candidate;
            }
        } catch {
            // ignore unreadable PATH entries
        }
    }
    return undefined;
}

/// Resolve how to launch the language server, in priority order:
///   1. `forgedb.lspServerPath` — an explicit `forgedb-lsp` binary;
///   2. the installed `forgedb` CLI (config `forgedb.path`, else on PATH),
///      launched via its `lsp` subcommand, which locates the sibling
///      `forgedb-lsp` binary itself (epic #173 WS4).
/// Returns undefined when no CLI/server can be found.
function resolveServer(): { command: string; args: string[] } | undefined {
    const config = vscode.workspace.getConfiguration('forgedb');

    const lspServerPath = config.get<string>('lspServerPath')?.trim();
    if (lspServerPath) {
        return { command: lspServerPath, args: [] };
    }

    const cli = resolveCli();
    if (cli) {
        return { command: cli, args: ['lsp'] };
    }

    return undefined;
}

/// Resolve the `forgedb` CLI command: explicit `forgedb.path` config, else the
/// binary on PATH. Returns undefined if neither is available.
function resolveCli(): string | undefined {
    const configured = vscode.workspace.getConfiguration('forgedb').get<string>('path')?.trim();
    if (configured) {
        return configured;
    }
    return findOnPath('forgedb');
}

/// The `forgedb` command to send to an integrated terminal — the resolved CLI
/// path (quoted if it contains spaces), falling back to a bare `forgedb` so the
/// terminal surfaces a clear "command not found" if it is genuinely absent.
function cliCommand(): string {
    const cli = resolveCli() ?? 'forgedb';
    return cli.includes(' ') ? `"${cli}"` : cli;
}

export async function activate(context: vscode.ExtensionContext) {
    console.log('ForgeDB extension is now active');

    // Create status bar item
    statusBarItem = vscode.window.createStatusBarItem(vscode.StatusBarAlignment.Left, 100);
    statusBarItem.text = "$(database) ForgeDB";
    statusBarItem.tooltip = "ForgeDB Language Server";
    statusBarItem.show();
    context.subscriptions.push(statusBarItem);

    // Start LSP server
    await startLanguageServer(context);

    // Register commands
    registerCommands(context);

    // Watch for schema file changes
    setupFileWatcher(context);

    updateStatusBar('Active', true);
}

async function startLanguageServer(context: vscode.ExtensionContext) {
    // The language server ships with the installed `forgedb` CLI (epic #173 WS4):
    // the extension bundles no binary and downloads nothing. It resolves the
    // server from the user's CLI so editor diagnostics stay in lockstep with the
    // CLI's own compiler (single source of truth — see #175).
    const server = resolveServer();
    if (!server) {
        updateStatusBar('CLI not found', false);
        const choice = await vscode.window.showWarningMessage(
            'ForgeDB CLI not found. The language server ships with the `forgedb` CLI — ' +
                'install it, or set `forgedb.path` / `forgedb.lspServerPath` in settings.',
            'Install instructions',
            'Open settings'
        );
        if (choice === 'Install instructions') {
            vscode.env.openExternal(vscode.Uri.parse('https://github.com/hoodiecollin/forgedb#installation'));
        } else if (choice === 'Open settings') {
            vscode.commands.executeCommand('workbench.action.openSettings', 'forgedb');
        }
        return;
    }

    const serverOptions: ServerOptions = {
        command: server.command,
        args: server.args,
        transport: TransportKind.stdio
    };

    const clientOptions: LanguageClientOptions = {
        documentSelector: [{ scheme: 'file', language: 'forge' }],
        synchronize: {
            fileEvents: vscode.workspace.createFileSystemWatcher('**/*.forge')
        }
    };

    client = new LanguageClient(
        'forgedbLanguageServer',
        'ForgeDB Language Server',
        serverOptions,
        clientOptions
    );
    // Tie the client's lifecycle to the extension so it is disposed on unload.
    context.subscriptions.push(client);

    try {
        await client.start();
        console.log('ForgeDB Language Server started successfully');
    } catch (error) {
        vscode.window.showErrorMessage(`Failed to start ForgeDB Language Server: ${error}`);
        console.error('LSP startup error:', error);
    }
}

function registerCommands(context: vscode.ExtensionContext) {
    // Command: Generate Code
    context.subscriptions.push(
        vscode.commands.registerCommand('forgedb.generateCode', async () => {
            const terminal = vscode.window.createTerminal('ForgeDB Generate');
            terminal.show();
            terminal.sendText(`${cliCommand()} generate`);
            vscode.window.showInformationMessage('Running ForgeDB code generation...');
        })
    );

    // Command: Validate Schema
    context.subscriptions.push(
        vscode.commands.registerCommand('forgedb.validateSchema', async () => {
            const editor = vscode.window.activeTextEditor;
            if (!editor || editor.document.languageId !== 'forge') {
                vscode.window.showWarningMessage('No .forge file is currently open');
                return;
            }

            // The LSP server already validates, so just trigger diagnostics
            await vscode.commands.executeCommand('vscode.executeDocumentSymbolProvider', editor.document.uri);
            vscode.window.showInformationMessage('Schema validation complete. Check Problems panel for issues.');
        })
    );

    // Command: Start Dev Mode (File Watcher)
    context.subscriptions.push(
        vscode.commands.registerCommand('forgedb.startDevMode', async () => {
            const terminal = vscode.window.createTerminal('ForgeDB Dev');
            terminal.show();
            terminal.sendText(`${cliCommand()} dev`);
            vscode.window.showInformationMessage('ForgeDB dev mode started. Watching for schema changes...');
        })
    );

    // Command: Create New Model
    context.subscriptions.push(
        vscode.commands.registerCommand('forgedb.createModel', async () => {
            const modelName = await vscode.window.showInputBox({
                prompt: 'Enter model name',
                placeHolder: 'User',
                validateInput: (value) => {
                    if (!value) return 'Model name is required';
                    if (!/^[A-Z][a-zA-Z0-9]*$/.test(value)) {
                        return 'Model name must start with uppercase letter and contain only alphanumeric characters';
                    }
                    return null;
                }
            });

            if (!modelName) return;

            const editor = vscode.window.activeTextEditor;
            if (!editor || editor.document.languageId !== 'forge') {
                vscode.window.showWarningMessage('Please open a .forge file first');
                return;
            }

            const snippet = new vscode.SnippetString();
            snippet.appendText(`\n\n${modelName} {\n`);
            snippet.appendText(`  id: +uuid\n`);
            snippet.appendPlaceholder('field_name');
            snippet.appendText(': ');
            snippet.appendPlaceholder('string');
            snippet.appendText('\n  created_at: &timestamp\n');
            snippet.appendText('}\n');

            editor.insertSnippet(snippet);
        })
    );

    // Command: Restart Language Server
    context.subscriptions.push(
        vscode.commands.registerCommand('forgedb.restartServer', async () => {
            if (client) {
                await client.stop();
                await startLanguageServer(context);
                vscode.window.showInformationMessage('ForgeDB Language Server restarted');
            }
        })
    );

    // Command: Show Output
    context.subscriptions.push(
        vscode.commands.registerCommand('forgedb.showOutput', () => {
            if (client) {
                client.outputChannel.show();
            }
        })
    );
}

function setupFileWatcher(context: vscode.ExtensionContext) {
    const config = vscode.workspace.getConfiguration('forgedb');
    const autoGenerateOnSave = config.get<boolean>('autoGenerateOnSave', false);

    if (!autoGenerateOnSave) return;

    const watcher = vscode.workspace.createFileSystemWatcher('**/*.forge');

    watcher.onDidChange(async () => {
        if (autoGenerateOnSave) {
            const terminal = vscode.window.createTerminal('ForgeDB Auto-Generate');
            terminal.sendText(`${cliCommand()} generate`);
            vscode.window.showInformationMessage('Auto-generating code from schema changes...');
        }
    });

    context.subscriptions.push(watcher);
}

function updateStatusBar(status: string, isActive: boolean) {
    if (isActive) {
        statusBarItem.text = `$(database) ForgeDB: ${status}`;
        statusBarItem.color = undefined;
    } else {
        statusBarItem.text = `$(database) ForgeDB: ${status}`;
        statusBarItem.color = new vscode.ThemeColor('statusBarItem.warningForeground');
    }
}

export async function deactivate() {
    if (client) {
        await client.stop();
    }
    statusBarItem?.dispose();
}
