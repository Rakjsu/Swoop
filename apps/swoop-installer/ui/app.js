// Instalador do Swoop: Bem-vindo → Instalando → Concluído | Erro.
// DOM puro, sem framework; fala com o Rust por window.__TAURI__.
'use strict';

const { invoke } = window.__TAURI__.core;
const appWindow = window.__TAURI__.window.getCurrentWindow();
const $ = (id) => document.getElementById(id);

const steps = ['welcome', 'installing', 'done', 'error'];
const STATUS = [
  'Copiando arquivos…',
  'Criando atalhos…',
  'Registrando o desinstalador…',
  'Preparando as atualizações automáticas…',
  'Quase lá…',
];

let installDir = '';
let statusTimer = null;

/** Mostra só a etapa pedida. */
function showStep(name) {
  for (const s of steps) $(`step-${s}`).classList.toggle('active', s === name);
}

function setDir(dir) {
  installDir = dir;
  $('dir-path').textContent = dir;
  $('dir-path').title = dir;
}

/** Troca a frase de status enquanto o setup roda (ele não informa progresso). */
function rotateStatus() {
  let i = 0;
  const el = $('install-status');
  el.textContent = STATUS[0];
  statusTimer = setInterval(() => {
    i = Math.min(i + 1, STATUS.length - 1);
    el.classList.add('fade');
    setTimeout(() => {
      el.textContent = STATUS[i];
      el.classList.remove('fade');
    }, 250);
  }, 2600);
}

function stopStatus() {
  clearInterval(statusTimer);
  statusTimer = null;
}

function showError(message) {
  stopStatus();
  $('error-detail').textContent = message.charAt(0).toUpperCase() + message.slice(1);
  showStep('error');
}

/** Roda o setup silencioso e decide a próxima etapa pelo código de saída. */
async function startInstall() {
  showStep('installing');
  rotateStatus();
  try {
    const code = await invoke('start_install', { dir: installDir });
    stopStatus();
    if (code === 0) {
      showStep('done');
    } else {
      showError(
        `A instalação não terminou (código ${code}). ` +
          'Feche o Swoop, se estiver aberto, e tente de novo.',
      );
    }
  } catch (err) {
    showError(String(err));
  }
}

invoke('installer_info').then((info) => {
  $('app-version').textContent = `Versão ${info.version}`;
  setDir(info.default_dir);
  if (!info.has_payload) {
    $('app-version').textContent += ' (build de desenvolvimento, sem pacote)';
  }
});

$('btn-install').addEventListener('click', startInstall);
$('btn-retry').addEventListener('click', startInstall);
$('btn-toggle-dir').addEventListener('click', () => $('dir-row').classList.toggle('hidden'));
$('btn-browse').addEventListener('click', async () => {
  const dir = await invoke('choose_dir', { current: installDir });
  if (dir) setDir(dir);
});
$('btn-finish').addEventListener('click', async () => {
  if ($('chk-launch').checked) {
    try {
      await invoke('launch_app', { dir: installDir });
    } catch (err) {
      showError(String(err));
      return;
    }
  }
  appWindow.close();
});
$('btn-minimize').addEventListener('click', () => appWindow.minimize());
$('btn-close').addEventListener('click', () => appWindow.close());
$('btn-error-close').addEventListener('click', () => appWindow.close());
