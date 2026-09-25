import { PauseIcon, PlayIcon, PlusIcon } from './icons';

const MIB = 1024 * 1024;

/** Opções do limite global (bytes/s; 0 = sem limite). */
const LIMITS: Array<[number, string]> = [
  [0, 'Sem limite'],
  [MIB / 2, '512 KB/s'],
  [MIB, '1 MB/s'],
  [2 * MIB, '2 MB/s'],
  [5 * MIB, '5 MB/s'],
  [10 * MIB, '10 MB/s'],
  [20 * MIB, '20 MB/s'],
];

interface Props {
  limitBps: number | null;
  onAdd: () => void;
  onPauseAll: () => void;
  onResumeAll: () => void;
  onLimit: (bps: number | null) => void;
}

/** Barra de cima: adicionar, pausar/retomar tudo e limite de velocidade. */
export function Toolbar({ limitBps, onAdd, onPauseAll, onResumeAll, onLimit }: Props) {
  const current = limitBps ?? 0;
  const known = LIMITS.some(([bps]) => bps === current);
  return (
    <div className="toolbar">
      <button type="button" className="btn btn-primary" onClick={onAdd}>
        <PlusIcon /> Adicionar links
      </button>
      <button type="button" className="btn" onClick={onPauseAll}>
        <PauseIcon /> Pausar tudo
      </button>
      <button type="button" className="btn" onClick={onResumeAll}>
        <PlayIcon /> Retomar tudo
      </button>
      <label className="toolbar-limit">
        Limite
        <select
          value={current}
          onChange={(e) => {
            const bps = Number(e.target.value);
            onLimit(bps > 0 ? bps : null);
          }}
        >
          {!known && <option value={current}>{`${(current / MIB).toFixed(1)} MB/s`}</option>}
          {LIMITS.map(([bps, label]) => (
            <option key={bps} value={bps}>
              {label}
            </option>
          ))}
        </select>
      </label>
    </div>
  );
}
