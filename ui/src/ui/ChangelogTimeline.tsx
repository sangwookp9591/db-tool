// Source: user-provided component (21st.dev "changelog-timeline", demo credits CCgather). React + Tailwind + lucide-react.
// AxDB port target: Preact + plain CSS + lucide-preact (no Tailwind/shadcn in this project).

import { Fragment, type ComponentChildren, type HTMLAttributes } from "preact";
import * as hooks from "preact/hooks";
import "./ChangelogTimeline.css";
import { ChevronDown, GitBranch, GitCommitVertical, Pin, X } from "lucide-preact";

export type ChangelogCategory = "new" | "improved" | "fixed";

export interface ChangelogEntry {
  id: string;
  /** ISO date, e.g. "2026-10-04". */
  date: string;
  category: ChangelogCategory;
  title: string;
  summary?: string;
  href?: string;
}

export interface ChangelogStatus {
  commit?: string;
  /** Branch or tag, shown as ref@commit. */
  ref?: string;
  deployedAt?: string | Date;
  note?: ComponentChildren;
}

type DivProps = HTMLAttributes<HTMLDivElement>;
type ContainerProps = { [Key in keyof DivProps as Key extends "title" ? never : Key]: DivProps[Key] };

export type ChangelogTimelineProps = ContainerProps & {
  entries: ChangelogEntry[];
  title?: ComponentChildren;
  status?: ChangelogStatus;
  /** Rows before the list scrolls; half of the next headline remains visible. */
  visibleEntries?: number;
  historyHref?: string;
  historyLabel?: string;
  onClose?: () => void;
  onOpen?: (url: string) => void;
  expandOnHover?: boolean;
  expanded?: string | null;
  defaultExpanded?: string | null;
  onExpandedChange?: (id: string | null) => void;
  /** A pinned row holds the single expansion until unpinned. */
  pinned?: string | null;
  defaultPinned?: string | null;
  onPinnedChange?: (id: string | null) => void;
};

const CATEGORY = {
  new: { label: "추가", mark: "+", color: "var(--accent-success)" },
  improved: { label: "개선", mark: "~", color: "var(--accent-info)" },
  fixed: { label: "수정", mark: "!", color: "var(--accent-warning)" },
} as const;

const MONTH_LABEL = new Intl.DateTimeFormat("ko-KR", { month: "long", year: "numeric", timeZone: "UTC" });

function cx(...classes: (string | false | null | undefined)[]) {
  return classes.filter(Boolean).join(" ");
}

function relativeTime(value: string | Date, now: number) {
  const then = typeof value === "string" ? Date.parse(value) : value.getTime();
  if (Number.isNaN(then)) return null;
  const minutes = Math.max(0, Math.round((now - then) / 60000));
  if (minutes < 60) return `${minutes}분 전`;
  const hours = Math.round(minutes / 60);
  return hours < 24 ? `${hours}시간 전` : `${Math.round(hours / 24)}일 전`;
}

export function ChangelogTimeline({
  entries,
  title = "새로운 기능",
  status,
  visibleEntries = 9,
  historyHref,
  historyLabel = "전체 릴리스 기록",
  onClose,
  onOpen,
  expandOnHover = true,
  expanded,
  defaultExpanded = null,
  onExpandedChange,
  pinned,
  defaultPinned = null,
  onPinnedChange,
  ref,
  className,
  ...rest
}: ChangelogTimelineProps) {
  const [localExpanded, setLocalExpanded] = hooks.useState(defaultExpanded ?? defaultPinned);
  const [localPinned, setLocalPinned] = hooks.useState(defaultPinned);
  const [layout, setLayout] = hooks.useState({ top: 0, height: 0, start: 0, length: 0, maxHeight: undefined as number | undefined });
  const stageRef = hooks.useRef(null as HTMLDivElement | null);
  const listRef = hooks.useRef(null as HTMLOListElement | null);
  const hoverFrame = hooks.useRef(0);
  const leaveTimer = hooks.useRef(0);
  const pendingHover = hooks.useRef(null as string | null);
  const titleId = hooks.useId();
  const idPrefix = hooks.useId();
  const [now] = hooks.useState(Date.now);
  const sorted = hooks.useMemo(() => entries.slice().sort((a, b) => a.date < b.date ? 1 : a.date > b.date ? -1 : 0), [entries]);
  const requestedExpanded = expanded === undefined ? localExpanded : expanded;
  const activeId = sorted.some((entry) => entry.id === requestedExpanded) ? requestedExpanded : null;
  const requestedPin = pinned === undefined ? localPinned : pinned;
  // A controlled expanded value remains authoritative, even if a pin intent is declined.
  const pinnedId = requestedPin === activeId ? activeId : null;
  const activeEntry = sorted.find((entry) => entry.id === activeId);
  const oldest = sorted[sorted.length - 1];
  const deployed = status?.deployedAt && now ? relativeTime(status.deployedAt, now) : null;
  const count = Number.isFinite(visibleEntries) ? Math.max(1, Math.floor(visibleEntries)) : 9;

  const updateExpanded = (id: string | null) => {
    if (id === activeId) return;
    if (expanded === undefined) setLocalExpanded(id);
    onExpandedChange?.(id);
  };
  const updatePinned = (id: string | null) => {
    if (pinned === undefined) setLocalPinned(id);
    onPinnedChange?.(id);
  };
  const actions = hooks.useRef({ updateExpanded, pinnedId, expandOnHover });
  hooks.useLayoutEffect(() => { actions.current = { updateExpanded, pinnedId, expandOnHover }; });

  const cancelPending = () => {
    window.clearTimeout(leaveTimer.current);
    window.cancelAnimationFrame(hoverFrame.current);
    hoverFrame.current = 0;
  };
  const preview = (id: string) => {
    window.clearTimeout(leaveTimer.current);
    if (!expandOnHover || pinnedId) return;
    pendingHover.current = id;
    if (hoverFrame.current) return;
    // Pointer moves commit once per frame; layout-induced pointerenter events cannot bounce rows.
    hoverFrame.current = window.requestAnimationFrame(() => {
      hoverFrame.current = 0;
      if (actions.current.expandOnHover && !actions.current.pinnedId) actions.current.updateExpanded(pendingHover.current);
    });
  };
  const leave = () => {
    window.cancelAnimationFrame(hoverFrame.current);
    hoverFrame.current = 0;
    window.clearTimeout(leaveTimer.current);
    leaveTimer.current = window.setTimeout(() => {
      const focused = listRef.current?.querySelector(":focus-visible");
      if (!actions.current.pinnedId && !focused) actions.current.updateExpanded(null);
    }, window.matchMedia("(prefers-reduced-motion: reduce)").matches ? 0 : 120);
  };
  const togglePin = (id: string) => {
    cancelPending();
    const next = pinnedId === id ? null : id;
    updatePinned(next);
    updateExpanded(next);
  };

  hooks.useEffect(() => () => {
    window.clearTimeout(leaveTimer.current);
    window.cancelAnimationFrame(hoverFrame.current);
  }, []);

  hooks.useLayoutEffect(() => {
    const stage = stageRef.current;
    const list = listRef.current;
    if (!stage || !list) return;
    let frame = 0;
    let live = true;
    const rows = Array.from(list.querySelectorAll("[data-entry]")) as HTMLLIElement[];
    const measure = () => {
      frame = 0;
      if (!live) return;
      const origin = stage.getBoundingClientRect().top;
      const firstNode = rows[0]?.querySelector("[data-node]")?.getBoundingClientRect();
      const lastNode = rows.at(-1)?.querySelector("[data-node]")?.getBoundingClientRect();
      const activeIndex = rows.findIndex((item) => item.dataset.entryId === activeId);
      const activeRow = rows[activeIndex];
      const row = activeRow?.getBoundingClientRect();
      const bodyHeight = activeRow?.querySelector("[data-entry-body]")?.getBoundingClientRect().height ?? 0;
      const fullHeight = activeRow?.querySelector("[data-entry-copy]")?.getBoundingClientRect().height ?? 0;
      const precedingHeight = rows.slice(0, Math.max(0, activeIndex)).reduce((height, item) => height + (item.querySelector("[data-entry-body]")?.getBoundingClientRect().height ?? 0), 0);
      const peek = rows[count];
      const peekBox = peek?.getBoundingClientRect();
      const peekHeader = peek?.querySelector("[data-entry-trigger]")?.getBoundingClientRect();
      const next = {
        // Target the final accordion geometry once, so body resizing cannot restart the rail transition.
        top: row ? row.top - origin - precedingHeight : 0,
        height: row ? row.height - bodyHeight + fullHeight : 0,
        start: firstNode ? firstNode.top + firstNode.height / 2 - origin : 0,
        length: firstNode && lastNode ? lastNode.top + lastNode.height / 2 - firstNode.top - firstNode.height / 2 : 0,
        maxHeight: peekBox && peekHeader ? peekBox.top - origin + peekHeader.height / 2 : undefined,
      };
      setLayout((previous) => Object.keys(next).every((key) => previous[key as keyof typeof next] === next[key as keyof typeof next]) ? previous : next);
    };
    const schedule = () => { if (!frame) frame = window.requestAnimationFrame(measure); };
    measure();
    const observer = new ResizeObserver(schedule);
    observer.observe(stage);
    rows.forEach((row) => observer.observe(row));
    document.fonts?.ready.then(schedule).catch(() => undefined);
    return () => { live = false; observer.disconnect(); window.cancelAnimationFrame(frame); };
  }, [activeId, entries, count]);

  let lastMonth = "";
  return (
    <div {...rest} ref={ref} role="region" aria-labelledby={titleId} data-slot="changelog-timeline" data-expanded={activeId ?? ""} data-pinned={pinnedId ?? ""} data-autoplay="false" className={cx("changelog-timeline", className as string)}>
      <div className="changelog-timeline__header">
        <div className="changelog-timeline__heading">
          <span aria-hidden="true" className="changelog-timeline__icon"><GitBranch className="changelog-timeline__icon-small" /></span>
          <div>
            <h2 id={titleId} className="changelog-timeline__title">{title}</h2>
            {oldest && <p className="changelog-timeline__subtitle">{sorted.length}개 업데이트 · {oldest.date ? `${oldest.date}부터` : "날짜 미상"}</p>}
          </div>
        </div>
        {onClose ? <button type="button" onClick={onClose} aria-label="닫기" className="changelog-timeline__close"><X className="changelog-timeline__icon-small" /></button> : <span className="changelog-timeline__hint">마우스를 올려 읽고 클릭해 고정</span>}
      </div>

      {status && <div className="changelog-timeline__status-wrap">
        <div data-live-status className="changelog-timeline__status">
          <span data-live-pulse aria-hidden="true" className="changelog-timeline__live-dot" />
          <span className="changelog-timeline__status-label">현재 버전</span>
          {(status.ref || status.commit) && <><span aria-hidden="true">/</span><span className="changelog-timeline__commit">{status.ref}{status.commit ? `@${status.commit}` : ""}</span></>}
          {deployed && <><span aria-hidden="true">·</span><span>배포 {deployed}</span></>}
          {status.note && <><span aria-hidden="true">·</span><span>{status.note}</span></>}
        </div>
      </div>}

      <div data-list-viewport className="changelog-timeline__viewport" style={{ maxHeight: layout.maxHeight }}>
        <div ref={stageRef} data-rail-stage className="changelog-timeline__stage">
          <span data-rail-base aria-hidden="true" className="changelog-timeline__rail" style={{ top: layout.start, height: layout.length }} />
          <span data-rail-highlight aria-hidden="true" className="changelog-timeline__highlight" style={{ transform: `translateY(${layout.top}px) scaleY(${layout.height})`, backgroundColor: activeEntry ? CATEGORY[activeEntry.category].color : "var(--accent-primary)", opacity: activeId ? 1 : 0 }} />
          <ol className="changelog-timeline__list" ref={listRef} data-entry-list onPointerMove={(event) => {
            if (event.pointerType === "touch") return;
            const target = (event.target as Element).closest("[data-entry]") as HTMLElement | null;
            if (target?.dataset.entryId) preview(target.dataset.entryId);
            else window.clearTimeout(leaveTimer.current);
          }} onPointerEnter={() => window.clearTimeout(leaveTimer.current)} onPointerLeave={leave} onBlur={(event) => {
            if (!event.currentTarget.contains(event.relatedTarget as Node | null)) leave();
          }}>
            {sorted.map((entry, index) => {
              const month = entry.date.slice(0, 7);
              const showMonth = month !== lastMonth;
              lastMonth = month;
              const date = new Date(`${entry.date}T00:00:00Z`);
              const monthLabel = Number.isNaN(date.getTime()) ? month || "날짜 미상" : MONTH_LABEL.format(date);
              const category = CATEGORY[entry.category];
              const open = activeId === entry.id;
              const isPinned = pinnedId === entry.id;
              const bodyId = `${idPrefix}-body-${index}`;
              const triggerId = `${idPrefix}-trigger-${index}`;
              return <Fragment key={entry.id}>
                {showMonth && <li data-month className="changelog-timeline__month">{monthLabel}</li>}
                <li data-entry data-entry-id={entry.id} data-category={entry.category} data-open={open} className="changelog-timeline__entry">
                  <button type="button" id={triggerId} data-entry-trigger={entry.id} aria-expanded={open} aria-pressed={isPinned} aria-controls={bodyId} title={entry.title} onFocus={() => {
                    window.clearTimeout(leaveTimer.current);
                    if (!pinnedId) updateExpanded(entry.id);
                  }} onClick={() => togglePin(entry.id)} onKeyDown={(event) => {
                    if (event.key === "Escape") { event.preventDefault(); event.stopPropagation(); cancelPending(); updatePinned(null); updateExpanded(null); return; }
                    const direction = event.key === "ArrowDown" ? 1 : event.key === "ArrowUp" ? -1 : 0;
                    if (!direction && event.key !== "Home" && event.key !== "End") return;
                    event.preventDefault();
                    const buttons = Array.from(listRef.current?.querySelectorAll("[data-entry-trigger]") ?? []) as HTMLButtonElement[];
                    const next = event.key === "Home" ? 0 : event.key === "End" ? buttons.length - 1 : (index + direction + buttons.length) % buttons.length;
                    buttons[next]?.focus();
                  }} className="changelog-timeline__trigger">
                    <span data-node aria-hidden="true" className="changelog-timeline__node" style={{ color: open ? category.color : "var(--text-muted)" }}>
                      {open && <span key={entry.id} data-node-ripple className="changelog-timeline__ripple" />}
                      <span data-node-core data-idle-pulse={index === 0 ? "true" : undefined} className="changelog-timeline__node-core" style={{ transform: open ? "scale(1.4)" : "scale(1)", opacity: open ? 1 : 0.45 }} />
                    </span>
                    <time dateTime={entry.date || undefined} className="changelog-timeline__date">{entry.date ? entry.date.slice(5).replace("-", ".") : "—"}</time>
                    <span className="changelog-timeline__category">{category.mark} {category.label}</span>
                    <span className="changelog-timeline__headline">{entry.title}</span>
                    {isPinned ? <Pin data-pin-mark aria-label="고정됨" className="changelog-timeline__pin" /> : <ChevronDown aria-hidden="true" className="changelog-timeline__chevron" />}
                  </button>
                  <div id={bodyId} className="changelog-timeline__body" role="region" aria-labelledby={triggerId} aria-hidden={!open} inert={!open} data-entry-body data-open={open}>
                    <div className="changelog-timeline__body-inner">
                      <div data-entry-copy className="changelog-timeline__copy">
                        {entry.summary && <p className="changelog-timeline__summary">{entry.summary}</p>}
                        {entry.href && <a href={entry.href} onClick={event => { if (onOpen) { event.preventDefault(); onOpen(entry.href!); } }} target="_blank" rel="noreferrer" className="changelog-timeline__read">{"자세히 보기"}<span className="sr-only">: {entry.title}</span></a>}
                        {!entry.summary && !entry.href && <p className="changelog-timeline__summary">{entry.title}</p>}
                      </div>
                    </div>
                  </div>
                </li>
              </Fragment>;
            })}
          </ol>
          {sorted.length === 0 && <p className="changelog-timeline__empty">아직 업데이트가 없습니다.</p>}
        </div>
      </div>

      {historyHref && <div className="changelog-timeline__footer">
        <a href={historyHref} onClick={event => { if (onOpen) { event.preventDefault(); onOpen(historyHref); } }} target="_blank" rel="noreferrer" className="changelog-timeline__history"><GitCommitVertical aria-hidden="true" className="changelog-timeline__history-icon" />{historyLabel}</a>
        <span className="changelog-timeline__hint">{pinnedId ? "고정됨 · 다시 클릭해 해제" : "편하게 살펴보세요"}</span>
      </div>}
      <p role="status" aria-live="polite" className="sr-only">{pinnedId ? `고정됨: ${activeEntry?.title}` : "고정된 업데이트가 없습니다."}</p>
    </div>
  );
}

export default ChangelogTimeline;
