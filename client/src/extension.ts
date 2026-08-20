import * as fs from 'fs';
import * as path from 'path';
import * as vscode from 'vscode';
import {
    LanguageClient,
    LanguageClientOptions,
    ServerOptions,
} from 'vscode-languageclient/node';

let client: LanguageClient | undefined;

/** Resolve the server binary: an explicit non-default setting wins,
 *  then the binary bundled inside platform-specific builds of this
 *  extension, then a PATH lookup. */
function serverCommand(context: vscode.ExtensionContext): string {
    const cfg = vscode.workspace.getConfiguration('kamailioLsp');
    const configured = cfg.get<string>('serverPath', 'kamailio-lsp');
    if (configured && configured !== 'kamailio-lsp') {
        return configured;
    }
    const exe = process.platform === 'win32' ? 'kamailio-lsp.exe' : 'kamailio-lsp';
    const bundled = path.join(context.extensionPath, 'server', exe);
    if (fs.existsSync(bundled)) {
        try {
            fs.chmodSync(bundled, 0o755);
        } catch {
            // read-only extension dir: the mode from the package applies
        }
        return bundled;
    }
    return configured;
}

function buildClient(context: vscode.ExtensionContext): LanguageClient {
    const cfg = vscode.workspace.getConfiguration('kamailioLsp');
    const diagnosticsWanted = cfg.get<boolean>('diagnostics.enable', true);
    // Trust gate: 'kamailio -c' dlopens the modules the cfg loads —
    // code execution. In untrusted workspaces diagnostics are forced
    // off regardless of settings; everything else keeps working.
    const kamailioPath =
        vscode.workspace.isTrusted && diagnosticsWanted
            ? cfg.get<string>('kamailioPath', 'kamailio')
            : '';
    const serverOptions: ServerOptions = {
        command: serverCommand(context),
    };
    const clientOptions: LanguageClientOptions = {
        documentSelector: [{ language: 'kamailio-cfg' }],
        initializationOptions: {
            kamailioPath,
            kamailioSrc: cfg.get<string>('kamailioSrc', ''),
            kamailioWiki: cfg.get<string>('kamailioWiki', ''),
            modulesPath: cfg.get<string>('modulesPath', ''),
            checkTimeoutMs: cfg.get<number>('checkTimeoutMs', 10000),
            snippetCompletions: cfg.get<boolean>('completion.snippets', true),
            analyzerDiagnostics: cfg.get<boolean>('diagnostics.analyzer', true),
            maxDiagnostics: cfg.get<number>('diagnostics.maxProblems', 100),
            cacheDir: cfg.get<string>('cacheDir', ''),
        },
    };
    return new LanguageClient(
        'kamailioLsp',
        'Kamailio LSP',
        serverOptions,
        clientOptions,
    );
}

async function restart(context: vscode.ExtensionContext): Promise<void> {
    if (client) {
        await client.stop();
    }
    client = buildClient(context);
    await client.start();
}

export function activate(context: vscode.ExtensionContext) {
    if (!vscode.workspace
        .getConfiguration('kamailioLsp')
        .get<boolean>('enable', true)) {
        return;
    }
    client = buildClient(context);
    client.start();
    // trust granted later: restart so diagnostics come alive with the
    // configured kamailioPath
    context.subscriptions.push(
        vscode.workspace.onDidGrantWorkspaceTrust(() => void restart(context)),
    );
    // settings changed: restart with the new configuration
    context.subscriptions.push(
        vscode.workspace.onDidChangeConfiguration((e) => {
            if (e.affectsConfiguration('kamailioLsp')) {
                void restart(context);
            }
        }),
    );
}

export function deactivate(): Thenable<void> | undefined {
    return client?.stop();
}
