import { useState } from 'preact/hooks';
import { invoke } from '@tauri-apps/api/core';
import { Download } from 'lucide-preact';
import { isMock } from './api';
import { Dialog } from './Dialog';
import { Button } from './ui/Button';
import { GlideSelect } from './ui/GlideSelect';
import { SpringCheck } from './ui/SpringCheck';
import { ToastStack, type ToastItem } from './ui/ToastStack';
import './export.css';

export interface ExportOptions { format: string; visible: boolean; nullLabel: boolean; background: boolean }
export interface ExportFile { bytes: Uint8Array; count: number; unit?: string }
const extensions: Record<string, string> = { CSV: 'csv', JSON: 'json', Markdown: 'md', NDJSON: 'ndjson', SVG: 'svg', PNG: 'png' };
const mime: Record<string, string> = { CSV: 'text/csv;charset=utf-8', JSON: 'application/json', Markdown: 'text/markdown;charset=utf-8', NDJSON: 'application/x-ndjson', SVG: 'image/svg+xml', PNG: 'image/png' };
export function ExportButton({ formats, name = 'axdb-results', count, truncated = false, table = false, image = false, disabled = false, makeFile }: { formats: string[]; name?: string; count: number; truncated?: boolean; table?: boolean; image?: boolean; disabled?: boolean; makeFile: (options: ExportOptions) => ExportFile | Promise<ExportFile> }) {
  const [open, setOpen] = useState(false), [busy, setBusy] = useState(false);
  const [format, setFormat] = useState(formats[0]), [visible, setVisible] = useState(true), [nullLabel, setNullLabel] = useState(false), [background, setBackground] = useState(true);
  const [toasts, setToasts] = useState<ToastItem[]>([]);
  const selected = formats.includes(format) ? format : formats[0];
  function fail(error: unknown) { setToasts(items => [...items, { id: crypto.randomUUID(), tone: 'error', message: `내보내기 실패: ${error instanceof Error ? error.message : typeof error === 'object' && error && 'message' in error ? String(error.message) : String(error)}` }]); }
  async function save() {
    if (busy) return;
    setBusy(true);
    try {
      const file = await makeFile({ format: selected, visible, nullLabel, background });
      let filename = `${name}.${extensions[selected]}`, receipt: string | undefined;
      if (isMock) {
        const blob = new Blob([file.bytes as BlobPart], { type: mime[selected] });
        const url = URL.createObjectURL(blob), link = document.createElement('a');
        link.href = url; link.download = filename; document.body.append(link); link.click(); link.remove();
        setTimeout(() => URL.revokeObjectURL(url), 60_000);
      } else {
        const grant = await invoke<{ path: string; token: string } | null>('export_save_dialog', { filename, extension: extensions[selected] });
        if (!grant) return;
        await invoke('export_write', { path: grant.path, token: grant.token, bytes: Array.from(file.bytes) });
        filename = grant.path.split(/[/\\]/).pop()!; receipt = grant.token;
      }
      setOpen(false);
      const id = crypto.randomUUID();
      setToasts(items => [...items, { id, tone: 'success', message: `${filename} · ${file.count.toLocaleString()}${file.unit ?? '행'} 저장 완료`, action: receipt ? { label: 'Finder에서 보기', onClick: () => { void invoke('export_reveal', { token: receipt }).catch(fail); } } : undefined }]);
    } catch (error) { fail(error); } finally { setBusy(false); }
  }
  return <><Button morph={false} icon={<Download />} disabled={disabled} aria-label={image ? '이미지 내보내기' : '결과 내보내기'} onClick={() => setOpen(true)}>{image ? '이미지 내보내기' : '내보내기'}</Button>
    {open && <Dialog label={image ? '이미지 내보내기' : '결과 내보내기'} closeLabel="내보내기 닫기" closeDisabled={busy} onClose={() => setOpen(false)} className="export-dialog" footer={<><Button morph={false} disabled={busy} onClick={() => setOpen(false)}>취소</Button><Button morph={false} loading={busy} onClick={() => void save()}>저장</Button></>}>
      <div class="export-options"><label class="export-format">파일 형식<GlideSelect ariaLabel="내보내기 형식" value={selected} options={formats} onChange={setFormat} /></label>
      <p>{truncated ? `불러온 ${count.toLocaleString()}행만 저장` : image ? '현재 캔버스 보기를 저장합니다.' : `현재 불러온 ${count.toLocaleString()}행을 저장합니다.`}</p>
      {table && <SpringCheck checked={visible} onChange={setVisible} label="필터와 정렬을 적용한 보이는 순서로 저장" />}
      {table && selected === 'CSV' && <SpringCheck checked={nullLabel} onChange={setNullLabel} label="NULL을 NULL 문자열로 저장 (기본: 빈 칸)" />}
      {image && <SpringCheck checked={background} onChange={setBackground} label="현재 테마의 배경 포함 (해제: 투명)" />}
      {!image && <small>저장된 DB 값만 포함하며, 적용 전 편집은 포함하지 않습니다. 큰 정수·numeric과 JSON 내부 숫자는 JSON에서 문자열로 보존합니다.</small>}
      {table && selected === 'CSV' && <small>빈 문자열은 ""로 저장합니다. 스프레드시트에서 수식으로 해석되는 텍스트는 가져오기 시 텍스트 형식으로 지정하세요.</small>}</div>
    </Dialog>}
    <ToastStack aria-label="내보내기 알림" items={toasts} onDismiss={id => setToasts(items => items.filter(item => item.id !== id))} />
  </>;
}
