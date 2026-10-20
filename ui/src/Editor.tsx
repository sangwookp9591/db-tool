import type { Diagnostic } from './ax/editorContract';
import { useEffect, useRef } from 'preact/hooks';
import { EditorState, Compartment, StateEffect, StateField, type Extension } from '@codemirror/state';
import { Decoration, EditorView, keymap, lineNumbers, highlightActiveLine, drawSelection } from '@codemirror/view';
import { defaultKeymap, history, historyKeymap, isolateHistory } from '@codemirror/commands';
import { defaultHighlightStyle, HighlightStyle, syntaxHighlighting } from '@codemirror/language';
const appliedRange=StateEffect.define<{from:number;to:number}|null>();
const appliedMark=StateField.define({create:()=>Decoration.none,update:(value,tr)=>{let next=value.map(tr.changes);for(const effect of tr.effects)if(effect.is(appliedRange))next=effect.value&&effect.value.to>effect.value.from?Decoration.set([Decoration.mark({class:'ai-applied-range'}).range(effect.value.from,effect.value.to)]):Decoration.none;return next;},provide:field=>EditorView.decorations.from(field)});
export interface EditorHandle { text: () => string; selection: () => string; insert: (text: string) => void; replace: (text: string) => void }
interface Props { diagnostics?: Diagnostic[]; onRefactor?: () => void; text: string; mode: string; onChange: (text: string) => void; onRun: () => void; onReady: (handle: EditorHandle | null) => void; onError: (error: unknown) => void }
export function Editor(props: Props) {
  const parent = useRef<HTMLDivElement>(null);
  const viewRef = useRef<EditorView | null>(null);
  const lint = useRef(new Compartment());
  useEffect(() => { const view=viewRef.current; if(!view)return; const marks=(props.diagnostics??[]).filter(d=>d.start>=0&&d.end>d.start&&d.end<=view.state.doc.length).map(d=>Decoration.mark({class:"ax-lint-underline",attributes:{title:d.message}}).range(d.start,d.end)); view.dispatch({effects:lint.current.reconfigure(EditorView.decorations.of(Decoration.set(marks,true)))}); }, [props.diagnostics]);
  const latest = useRef(props); latest.current = props;
  useEffect(() => {
    if (!parent.current) return;
    let disposed = false;
    let highlightTimer:ReturnType<typeof setTimeout>|undefined;
    const language = new Compartment();
    const view = new EditorView({ parent: parent.current, state: EditorState.create({ doc: latest.current.text, extensions: [
      lineNumbers(), history(), appliedMark, drawSelection({ cursorBlinkRate: 0 }), highlightActiveLine(), syntaxHighlighting(HighlightStyle.define(defaultHighlightStyle.specs.map(spec => spec.color ? { ...spec, color: `var(--syntax-${String(spec.color).slice(1)}, ${spec.color})` } : spec))), language.of([]), lint.current.of([]),
      EditorView.domEventHandlers({contextmenu: event => { if (!latest.current.onRefactor) return false; event.preventDefault(); latest.current.onRefactor(); return true; }}),
      EditorView.contentAttributes.of({ 'aria-label': 'Query editor', spellcheck: 'false' }),
      keymap.of([{ key: 'Mod-.', run: () => { latest.current.onRefactor?.(); return !!latest.current.onRefactor; } }, { key: 'Mod-Enter', run: () => { latest.current.onRun(); return true; } }, ...defaultKeymap, ...historyKeymap]),
      EditorView.updateListener.of(update => { if (update.docChanged) latest.current.onChange(update.state.doc.toString()); if(update.docChanged||update.selectionSet)window.dispatchEvent(new Event('ax:work-context')); }),
      EditorView.theme({ '&': { height: '100%', fontSize: '12.5px', background: 'var(--surface)', color: 'var(--text)' }, '.cm-scroller': { overflow: 'auto', fontFamily: 'var(--mono)' }, '.cm-content': { caretColor: 'var(--text)' }, '.cm-gutters': { background: 'var(--panel)', color: 'var(--muted)', borderRight: '1px solid var(--border)' }, '.cm-activeLine': { background: 'var(--highlight)' }, '.cm-selectionBackground, &.cm-focused .cm-selectionBackground': { background: 'var(--selection)' } })
    ] }) });
    viewRef.current = view;
    const apply=(text:string,whole=false)=>{const range=view.state.selection.main;const from=whole?0:range.from,to=whole?view.state.doc.length:range.to;view.dispatch({changes:{from,to,insert:text},selection:{anchor:from+text.length},annotations:isolateHistory.of('full'),effects:appliedRange.of({from,to:from+text.length})});clearTimeout(highlightTimer);highlightTimer=setTimeout(()=>{if(!disposed)view.dispatch({effects:appliedRange.of(null)});},1200);view.focus();};
    latest.current.onReady({ text: () => view.state.doc.toString(), selection: () => { const range = view.state.selection.main; return view.state.sliceDoc(range.from, range.to); }, insert: text => apply(text), replace: text => apply(text,true) });
    const loadLanguage = async (): Promise<Extension> => {
      if (props.mode === 'sql') return (await import('@codemirror/lang-sql')).sql();
      if (props.mode === 'cypher') return (await import('./cypher')).cypher();
      return [];
    };
    void loadLanguage().then(extension => { if (!disposed) view.dispatch({ effects: language.reconfigure(extension) }); }).catch(error => { if (!disposed) latest.current.onError(error); });
    return () => { disposed = true; clearTimeout(highlightTimer); latest.current.onReady(null); viewRef.current=null; view.destroy(); };
  }, [props.mode]);
  return <div class="editor" ref={parent} />;
}
