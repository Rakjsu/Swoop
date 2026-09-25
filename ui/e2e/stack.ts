import { spawn, type ChildProcess } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = dirname(fileURLToPath(import.meta.url));

/** Binários do cargo (debug por padrão; `SWOOP_BIN_DIR` troca). */
const BIN_DIR = resolve(process.env.SWOOP_BIN_DIR ?? join(HERE, '..', '..', 'target', 'debug'));
const EXE = process.platform === 'win32' ? '.exe' : '';

export interface Stack {
  /** Endereço do painel com o token (`http://127.0.0.1:P/#t=…`). */
  panelUrl: string;
  /** Base do servidor de teste (`http://127.0.0.1:P`). */
  testsrv: string;
  /** Pasta temporária (dados do serviço e downloads). */
  dir: string;
  stop(): Promise<void>;
}

/** Sobe o servidor de teste e o painel numa pasta temporária. */
export async function startStack(): Promise<Stack> {
  const dir = mkdtempSync(join(tmpdir(), 'swoop-e2e-'));
  const srv = spawn(join(BIN_DIR, `swoop-testsrv${EXE}`), ['0'], { stdio: ['ignore', 'pipe', 'inherit'] });
  const srvLine = await firstLine(srv, /http:\/\/[\d.:]+/);
  const serve = spawn(
    join(BIN_DIR, `swoop-cli${EXE}`),
    ['serve', '--port', '0', '--ui', resolve(HERE, '..', 'dist'), '--data-dir', join(dir, 'dados')],
    { stdio: ['ignore', 'pipe', 'inherit'] },
  );
  const panelUrl = await firstLine(serve, /http:\/\/127\.0\.0\.1:\d+\/#t=[0-9a-f]{64}/);
  return {
    panelUrl,
    testsrv: srvLine,
    dir,
    async stop() {
      await stopProcess(serve);
      await stopProcess(srv);
      rmSync(dir, { recursive: true, force: true });
    },
  };
}

/** Primeiro trecho do stdout que casa com o padrão (espera até 30 s). */
function firstLine(child: ChildProcess, pattern: RegExp): Promise<string> {
  return new Promise((ok, fail) => {
    let seen = '';
    const timer = setTimeout(() => fail(new Error(`sem saída esperada; visto: ${seen}`)), 30_000);
    child.stdout?.on('data', (chunk: Buffer) => {
      seen += chunk.toString();
      const match = pattern.exec(seen);
      if (match) {
        clearTimeout(timer);
        ok(match[0]);
      }
    });
    child.on('exit', (code) => fail(new Error(`processo saiu (${code}); visto: ${seen}`)));
  });
}

/** Ctrl+C (o serve grava o progresso) e espera sair. */
function stopProcess(child: ChildProcess): Promise<void> {
  return new Promise((ok) => {
    if (child.exitCode !== null) return ok();
    child.on('exit', () => ok());
    child.kill('SIGINT');
    setTimeout(() => child.kill('SIGKILL'), 10_000);
  });
}
