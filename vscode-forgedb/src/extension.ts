// ForgeDB VSCode Extension
// Integrates syntax highlighting, LSP, and commands

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
    const config = vscode.workspace.getConfiguration('forgedb');
    const lspServerPath = config.get<string>('lspServerPath');

    // Find the LSP server binary
    let serverCommand: string;

    if (lspServerPath) {
        serverCommand = lspServerPath;
    } else {
        // Try to find the compiled LSP server in the workspace
        const workspaceFolder = vscode.workspace.workspaceFolders?.[0];
        if (workspaceFolder) {
            // Try debug build first, then release
            const debugPath = path.join(workspaceFolder.uri.fsPath, 'target', 'debug', 'forgedb-lsp');
            const releasePath = path.join(workspaceFolder.uri.fsPath, 'target', 'release', 'forgedb-lsp');

            // Check if either exists
            const fs = require('fs');
            if (fs.existsSync(debugPath)) {
                serverCommand = debugPath;
            } else if (fs.existsSync(releasePath)) {
                serverCommand = releasePath;
            } else {
                vscode.window.showWarningMessage(
                    'ForgeDB LSP server not found. Please build it with: cargo build -p forgedb-lsp-server'
                );
                return;
            }
        } else {
            vscode.window.showWarningMessage('No workspace folder found');
            return;
        }
    }

    const serverOptions: ServerOptions = {
        command: serverCommand,
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
            terminal.sendText('cargo run --bin forgedb -- generate');
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
            terminal.sendText('cargo run --bin forgedb -- watch');
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

    watcher.onDidSave(async (uri) => {
        if (autoGenerateOnSave) {
            const terminal = vscode.window.createTerminal('ForgeDB Auto-Generate');
            terminal.sendText('cargo run --bin forgedb -- generate');
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
