import { useRef, useState } from 'preact/hooks';
import { invoke } from '@tauri-apps/api/core';
import { Download, FolderOpen, RefreshCw, ShieldAlert, X, Plus } from 'lucide-preact';
import { isMock } from '../api';
import { Button } from '../ui/Button';
import { JellyRadio } from '../ui/JellyRadio';
import { SpringCheck } from '../ui/SpringCheck';
import { Dialog } from '../Dialog';
import { changeDriver, driverError, refreshDrivers, useDrivers } from './api';
import { DriverProgress } from './DriverCard';
import { DriverGallery, DriversSkeleton } from './DriverGallery';
import { InstallStatus } from './InstallStatus';
import './drivers.css';

export function DriversPanel({ onClose, onAddConnection, requestedId }: { onClose: () => void; onAddConnection: () => void; requestedId?: string }) {
  const { catalog, busy, progress } = useDrivers();
  const [tab, setTab] = useState('available');
  const [fileError, setFileError] = useState(''), [unverified, setUnverified] = useState<string | null>(null), [consent, setConsent] = useState(false);
  const file = useRef<HTMLInputElement>(null);
  const installed = catalog.drivers.filter(driver => driver.installed_version);
  const visible = catalog.drivers.filter(driver => tab === 'installed' ? !!driver.installed_version : !driver.installed_version);
  const sorted = [...visible].sort((a, b) => Number(b.id === requestedId) - Number(a.id === requestedId));
  async function action(kind: 'install' | 'update' | 'remove', source: string, trusted = false) {
    setFileError('');
    try { await changeDriver(kind, source, trusted); if (kind !== 'remove') setTab('installed'); setUnverified(null); setConsent(false); }
    catch (error) {
      const message = driverError(error);
      if (kind === 'install' && message.includes('explicit consent required')) { setUnverified(source); setConsent(false); }
      else if (!/cancelled/i.test(message)) setFileError(message);
    }
  }
  async function chooseFile() {
    setFileError('');
    if (isMock) { file.current?.click(); return; }
    try {
      const path = await invoke<string | null>('driver_pick_file');
      if (path) await action('install', path);
    } catch (error) { setFileError(driverError(error)); }
  }
  return <section class="drivers-panel" aria-label="드라이버 관리">
    <header class="drivers-heading"><span class="drivers-emblem"><Download /></span><div><h1>드라이버</h1><p>필요한 데이터베이스만 설치하고 연결하세요.</p></div><Button morph={false} aria-label="드라이버 화면 닫기" onClick={onClose}><X /></Button></header>
    <div class="drivers-toolbar"><JellyRadio segmented role="tablist" aria-label="드라이버 목록" value={tab} onChange={setTab} items={[{ value: 'installed', label: <>설치됨 <span class="drivers-count">{installed.length}</span></> }, { value: 'available', label: '사용 가능' }]} /><span class="toolbar-spacer" /><Button morph={false} disabled={!!busy} onClick={() => void chooseFile()}><FolderOpen />파일에서 설치</Button><Button morph={false} disabled={catalog.refreshing} aria-label="드라이버 목록 새로고침" onClick={() => void refreshDrivers()}><RefreshCw /></Button></div>
    <div class="drivers-meta"><small role="status">{catalog.refreshing ? '저장된 목록 표시 · 최신 목록 확인 중…' : `${tab === 'installed' ? '설치한' : '사용 가능한'} 드라이버 ${visible.length}개`}</small>{isMock && <small class="mock-badge">모의 · 실제 설치 없음</small>}</div>
    {fileError && <p class="driver-error" role="alert">{fileError}</p>}
    {catalog.warning && <section class="drivers-catalog-status" aria-label="드라이버 저장소 상태"><ShieldAlert aria-hidden="true" /><div><h2>드라이버 저장소에 연결할 수 없습니다</h2><p>{catalogMessage(catalog.warning)}</p>{catalog.drivers.length > 0 && <p>저장된 목록을 표시합니다. 설치하려면 인터넷 연결이 필요합니다.</p>}<div class="drivers-state-actions"><Button morph={false} disabled={catalog.refreshing} onClick={() => void refreshDrivers()}><RefreshCw />다시 시도</Button><Button morph={false} disabled={!!busy} onClick={() => void chooseFile()}><FolderOpen />파일에서 설치</Button></div><details><summary>기술 상세</summary><pre>{catalog.warning}</pre></details></div></section>}
    <div class="drivers-grid" role="tabpanel" aria-label={tab === 'installed' ? '설치된 드라이버' : '설치 가능한 드라이버'}>{sorted.length > 0 && <DriverGallery drivers={sorted} scope="panel" onAction={(kind, id) => void action(kind, id)} onAddConnection={onAddConnection} />}{!catalog.drivers.length && catalog.refreshing ? <DriversSkeleton /> : !visible.length && <div class="drivers-empty"><Download /><h2>{tab === 'installed' ? '아직 설치한 드라이버가 없습니다' : catalog.warning ? '표시할 드라이버가 없습니다' : '새로 설치할 드라이버가 없습니다'}</h2><p>{tab === 'installed' ? '사용 가능 탭에서 필요한 드라이버를 설치하세요.' : '목록을 새로고침하거나 .axdriver 파일을 선택하세요.'}</p><div class="drivers-state-actions">{tab === 'installed' && <Button morph={false} onClick={() => setTab('available')}>사용 가능한 드라이버 보기</Button>}<Button morph={false} disabled={!!busy} onClick={() => void chooseFile()}><FolderOpen />파일에서 설치</Button><Button morph={false} disabled={catalog.refreshing} onClick={() => void refreshDrivers()}><RefreshCw />새로고침</Button></div></div>}</div>
    <InstallStatus compact onRetry={(kind, source) => void action(kind, source)} />
    <footer class="drivers-footer"><small>드라이버는 필요할 때만 실행됩니다.</small><Button morph={false} variant="primary" disabled={!installed.length || !!busy} onClick={onAddConnection}><Plus />연결 추가</Button></footer>
    <input hidden ref={file} type="file" accept=".axdriver" aria-label="드라이버 파일" onChange={e => { const selected = e.currentTarget.files?.[0]; e.currentTarget.value = ''; if (selected) void action('install', `/mock/${selected.name}`); }} />

    {unverified && <Dialog label="확인되지 않은 드라이버" onClose={() => { if (!busy) { setUnverified(null); setConsent(false); setFileError(''); } }} closeDisabled={!!busy} footer={<><Button morph={false} disabled={!!busy} onClick={() => { setUnverified(null); setConsent(false); }}>취소</Button><Button morph={false} variant="destructive" disabled={!consent || !!busy} onClick={() => void action('install', unverified, true)}>동의하고 설치</Button></>}><div class="driver-warning"><ShieldAlert /><p>이 파일은 공식 서명을 확인할 수 없습니다. 드라이버는 이 컴퓨터에서 실행되며 연결 정보와 데이터에 접근할 수 있습니다.</p><code>{unverified.split(/[/\\]/).pop()}</code><SpringCheck checked={consent} disabled={!!busy} onChange={setConsent} label="출처를 신뢰하며 위험을 이해했습니다" />{busy && <DriverProgress progress={progress} />}{fileError && <p class="driver-error" role="alert">{fileError}</p>}</div></Dialog>}
  </section>;
}

function catalogMessage(message: string) {
  if (/signature|signing|trusted|서명/i.test(message)) return '목록의 공식 서명을 확인하지 못했습니다. 검증되지 않은 목록은 사용하지 않습니다.';
  if (/404|not found/i.test(message)) return '공식 드라이버 목록이 아직 게시되지 않았거나 이동했습니다. 잠시 후 다시 시도해 주세요.';
  if (/timed? ?out|timeout|시간 초과/i.test(message)) return '서버 응답 시간이 초과되었습니다. 잠시 후 다시 시도해 주세요.';
  if (/offline|network|connect|dns|sending request|오프라인/i.test(message)) return '인터넷 연결을 확인해 주세요. 오프라인에서는 .axdriver 파일로 설치할 수 있습니다.';
  return '최신 목록을 불러오지 못했습니다. 다시 시도하거나 .axdriver 파일로 설치해 주세요.';
}
