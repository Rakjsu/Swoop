import { useEffect, useState } from 'react';
import type { Settings } from '../gen/Settings';
import { errorMessage, type Transport } from '../transport';
import './options.css';

const MIB = 1024 * 1024;

type Status = { kind: 'ok' | 'error'; text: string } | null;

/** Tela de opções: pastas por tipo, limites e avisos. Salva e aplica na hora. */
export function OptionsView({ transport }: { transport: Transport }) {
  const [form, setForm] = useState<Settings | null>(null);
  const [status, setStatus] = useState<Status>(null);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    transport
      .settings()
      .then(setForm)
      .catch((err: unknown) => setStatus({ kind: 'error', text: errorMessage(err) }));
  }, [transport]);

  if (!form) {
    return <p className="opt-loading">{status?.text ?? 'Carregando as preferências…'}</p>;
  }
  const set = <K extends keyof Settings>(key: K, value: Settings[K]) => {
    setForm({ ...form, [key]: value });
    setStatus(null);
  };

  const save = async () => {
    setSaving(true);
    try {
      await transport.exec({ type: 'save_settings', settings: form });
      setForm(await transport.settings());
      setStatus({ kind: 'ok', text: 'Preferências salvas.' });
    } catch (err) {
      setStatus({ kind: 'error', text: errorMessage(err) });
    } finally {
      setSaving(false);
    }
  };

  const limitMb = form.speed_limit_bps ? form.speed_limit_bps / MIB : 0;
  return (
    <section className="options">
      <h2>Pastas</h2>
      <p className="opt-hint">
        Sem pasta escolhida ao adicionar, cada arquivo vai para a pasta do seu tipo.
      </p>
      <FolderField label="Vídeos" value={form.video_dir} transport={transport} onChange={(v) => set('video_dir', v)} />
      <FolderField label="Músicas" value={form.music_dir} transport={transport} onChange={(v) => set('music_dir', v)} />
      <FolderField
        label="Outros arquivos"
        value={form.download_dir}
        transport={transport}
        onChange={(v) => set('download_dir', v)}
      />

      <h2>Downloads</h2>
      <NumberField label="Downloads ao mesmo tempo" min={1} max={20} value={form.max_active_downloads} onChange={(v) => set('max_active_downloads', v)} />
      <NumberField label="Conexões por arquivo" min={1} max={32} value={form.connections_per_download} onChange={(v) => set('connections_per_download', v)} />
      <NumberField label="Conexões por servidor" min={1} max={64} value={form.max_connections_per_host} onChange={(v) => set('max_connections_per_host', v)} />
      <NumberField label="Novas tentativas" min={0} max={50} value={form.max_retries} onChange={(v) => set('max_retries', v)} />
      <NumberField
        label="Limite de velocidade (MB/s, 0 = sem limite)"
        min={0}
        max={10000}
        step={0.5}
        value={limitMb}
        onChange={(v) => set('speed_limit_bps', v > 0 ? Math.round(v * MIB) : null)}
      />

      <h2>Avisos</h2>
      <label className="opt-check">
        <input type="checkbox" checked={form.notify} onChange={(e) => set('notify', e.target.checked)} />
        Avisar quando a fila terminar ou um download falhar
      </label>

      <div className="opt-actions">
        <button type="button" className="btn btn-primary" disabled={saving} onClick={save}>
          Salvar
        </button>
        {status && <span className={`opt-status opt-${status.kind}`}>{status.text}</span>}
      </div>
    </section>
  );
}

interface FolderProps {
  label: string;
  value: string | null;
  transport: Transport;
  onChange: (value: string | null) => void;
}

/** Pasta: texto editável + "Escolher…" no desktop. Vazio = padrão do sistema. */
function FolderField({ label, value, transport, onChange }: FolderProps) {
  const pick = async () => {
    const dir = await transport.pickFolder?.();
    if (dir) onChange(dir);
  };
  return (
    <label className="opt-field">
      <span>{label}</span>
      <div className="opt-folder">
        <input type="text" value={value ?? ''} placeholder="Padrão do sistema" onChange={(e) => onChange(e.target.value || null)} />
        {transport.pickFolder && (
          <button type="button" className="btn" onClick={pick}>
            Escolher…
          </button>
        )}
      </div>
    </label>
  );
}

interface NumberProps {
  label: string;
  min: number;
  max: number;
  step?: number;
  value: number;
  onChange: (value: number) => void;
}

/** Número com limites; valor inválido é ignorado até virar número. */
function NumberField({ label, min, max, step = 1, value, onChange }: NumberProps) {
  return (
    <label className="opt-field opt-number">
      <span>{label}</span>
      <input
        type="number"
        min={min}
        max={max}
        step={step}
        value={value}
        onChange={(e) => {
          const n = Number(e.target.value);
          if (e.target.value !== '' && Number.isFinite(n)) onChange(Math.min(max, Math.max(min, n)));
        }}
      />
    </label>
  );
}
