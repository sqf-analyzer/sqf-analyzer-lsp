/* --------------------------------------------------------------------------------------------
 * Copyright (c) Microsoft Corporation. All rights reserved.
 * Licensed under the MIT License. See License.txt in the project root for license information.
 * ------------------------------------------------------------------------------------------ */

import {
  workspace,
  ExtensionContext,
  window,
  WorkspaceConfiguration,
} from "vscode";
const path = require("path");
const fs = require("fs");

import {
  Executable,
  LanguageClient,
  LanguageClientOptions,
  ServerOptions,
} from "vscode-languageclient/node";

let client: LanguageClient;
// type a = Parameters<>;

function fileExists(path: string): boolean {
  try {
    fs.accessSync(path);
    return true;
  } catch (error) {
    return false;
  }
}

function getServer(conf: WorkspaceConfiguration): string {
  const windows = process.platform === "win32";
  const suffix = windows ? ".exe" : "";
  const binaryName = "sqf-analyzer-server" + suffix;

  console.log(binaryName);
  console.log(__dirname);
  const bundledPath = path.resolve(__dirname, binaryName);

  console.log(bundledPath);
  if (fileExists(bundledPath)) {
    return bundledPath;
  }

  return binaryName;
}


export async function activate(context: ExtensionContext) {
  const traceOutputChannel = window.createOutputChannel("SQF Language Server trace");

  const config = workspace.getConfiguration("sqf-analyzer");
  const command = process.env.SERVER_PATH || getServer(config);
  const run: Executable = {
    command,
    options: {
      env: {
        ...process.env,
        // eslint-disable-next-line @typescript-eslint/naming-convention
        RUST_LOG: "debug",
      },
    },
  };
  const serverOptions: ServerOptions = {
    run,
    debug: run,
  };
  let clientOptions: LanguageClientOptions = {
    // Register the server for plain text documents
    documentSelector: [{ scheme: "file", language: "sqf" }],
    synchronize: {
      // Notify the server about file changes to '.clientrc files contained in the workspace
      fileEvents: workspace.createFileSystemWatcher("**/.clientrc"),
    },
    traceOutputChannel,
  };

  // Create the language client and start the client.
  client = new LanguageClient("sqf-analyzer-server", "sqf analyzer server", serverOptions, clientOptions);
  client.start();
}

export function deactivate(): Thenable<void> | undefined {
  if (!client) {
    return undefined;
  }
  return client.stop();
}
