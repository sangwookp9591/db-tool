import type { Ref } from 'preact';
import { useId, useRef, useState } from 'preact/hooks';
import { Button } from './Button';
import './file-picker.css';

export interface PickedFile { name: string; size?: number }
export const formatFileSize = (bytes?: number) => bytes === undefined ? '' : bytes < 1024 ? `${bytes} B` : bytes < 1024 * 1024 ? `${(bytes / 1024).toFixed(1)} KB` : `${(bytes / 1024 / 1024).toFixed(1)} MB`;

/** 화면에 보이지 않는 네이티브 file input. 같은 파일을 다시 고를 수 있게 선택 직후 value를 비운다. */
export function FileTrigger({ inputRef, accept, multiple, disabled, ariaLabel, id, onFiles }: { inputRef?: Ref<HTMLInputElement>; accept?: string; multiple?: boolean; disabled?: boolean; ariaLabel?: string; id?: string; onFiles: (files: File[]) => void }) {
  return <input hidden ref={inputRef} id={id} type="file" accept={accept} multiple={multiple} disabled={disabled} aria-label={ariaLabel} onChange={event => { const files = Array.from(event.currentTarget.files ?? []); event.currentTarget.value = ''; if (files.length) onFiles(files); }} />;
}

export interface FilePickerProps {
  /** 접근 가능한 이름(숨은 input의 aria-label). 보이는 라벨이 있으면 같은 문구를 넣는다. */
  ariaLabel: string;
  accept?: string; multiple?: boolean; disabled?: boolean;
  /** 지정하면 표시 목록을 호출한 쪽이 관리한다. 생략하면 선택한 파일을 내부에 보관한다. */
  files?: readonly PickedFile[];
  onFiles?: (files: File[]) => void;
  /** 브라우저 파일 창 대신 네이티브 선택기를 쓸 때(Tauri 등). */
  onBrowse?: () => void;
  onClear?: () => void;
  buttonText?: string; emptyText?: string;
  class?: string;
}

export function FilePicker({ ariaLabel, accept, multiple, disabled, files, onFiles, onBrowse, onClear, buttonText = '파일 선택', emptyText = '선택된 파일 없음', class: className }: FilePickerProps) {
  const input = useRef<HTMLInputElement>(null), id = useId(), [inner, setInner] = useState<readonly PickedFile[]>([]), [drag, setDrag] = useState(false);
  const shown = files ?? inner;
  const take = (list: File[]) => {
    if (disabled || !list.length) return;
    const next = multiple ? list : list.slice(0, 1);
    if (files === undefined) setInner(next.map(file => ({ name: file.name, size: file.size })));
    onFiles?.(next);
  };
  const browse = () => { if (onBrowse) onBrowse(); else input.current?.click(); };
  const clear = () => { if (files === undefined) setInner([]); onClear?.(); };
  return <div class={`file-picker ${className ?? ''}`} data-drag={drag || undefined} data-disabled={disabled || undefined}
    onDragOver={event => { if (disabled) return; event.preventDefault(); if (!drag) setDrag(true); }}
    onDragLeave={event => { if (!event.currentTarget.contains(event.relatedTarget as Node | null)) setDrag(false); }}
    onDrop={event => { event.preventDefault(); setDrag(false); take(Array.from(event.dataTransfer?.files ?? [])); }}>
    <FileTrigger inputRef={input} id={`${id}-input`} accept={accept} multiple={multiple} disabled={disabled} ariaLabel={ariaLabel} onFiles={take} />
    <Button morph={false} disabled={disabled} aria-describedby={`${id}-status`} onClick={browse}>{buttonText}</Button>
    <span class="file-picker__status" id={`${id}-status`} data-empty={shown.length ? undefined : ''}>
      {shown.length ? shown.map(file => <span class="file-picker__file" key={file.name} title={file.name}><span class="file-picker__name">{file.name}</span>{file.size !== undefined && <small>{formatFileSize(file.size)}</small>}</span>) : <span class="file-picker__empty">{emptyText}</span>}
    </span>
    {shown.length > 0 && <Button morph={false} variant="ghost" disabled={disabled} aria-label={`${shown[0].name} 제거`} onClick={clear}>제거</Button>}
  </div>;
}
