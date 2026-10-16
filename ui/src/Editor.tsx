import { useEffect, useRef } from 'preact/hooks';
import { EditorState, Compartment, type Extension } from '@codemirror/state';
import { EditorView, keymap, lineNumbers, highlightActiveLine, drawSelection } from '@codemirror/view';
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
import { defaultHighlightStyle, HighlightStyle, syntaxHighlighting } from '@codemirror/language';
export interface EditorHandle { text: () => string; selection: () => string; insert: (text: string) => void; replace: (text: string) => void }
interface Props { text: string; mode: string; onChange: (text: string) => void; onRun: () => void; onReady: (handle: EditorHandle | null) => void; onError: (error: unknown) => void }
export function Editor(props: Props) {
  const parent = useRef<HTMLDivElement>(null);
  const latest = useRef(props); latest.current = props;
  useEffect(() => {
    if (!parent.current) return;
    let disposed = false;
    const language = new Compartment();
    const view = new EditorView({ parent: parent.current, state: EditorState.create({ doc: latest.current.text, extensions: [
      lineNumbers(), history(), drawSelection({ cursorBlinkRate: 0 }), highlightActiveLine(), syntaxHighlighting(HighlightStyle.define(defaultHighlightStyle.specs.map(spec => spec.color ? { ...spec, color: `var(--syntax-${String(spec.color).slice(1)}, ${spec.color})` } : spec))), language.of([]),
      EditorView.contentAttributes.of({ 'aria-label': 'Query editor', spellcheck: 'false' }),
      keymap.of([{ key: 'Mod-Enter', run: () => { latest.current.onRun(); return true; } }, ...defaultKeymap, ...historyKeymap]),
      EditorView.updateListener.of(update => { if (update.docChanged) latest.current.onChange(update.state.doc.toString()); }),
      EditorView.theme({ '&': { height: '100%', fontSize: '12.5px', background: 'var(--surface)', color: 'var(--text)' }, '.cm-scroller': { overflow: 'auto', fontFamily: 'var(--mono)' }, '.cm-content': { caretColor: 'var(--text)' }, '.cm-gutters': { background: 'var(--panel)', color: 'var(--muted)', borderRight: '1px solid var(--border)' }, '.cm-activeLine': { background: 'var(--highlight)' }, '.cm-selectionBackground, &.cm-focused .cm-selectionBackground': { background: 'var(--selection)' } })
    ] }) });
    latest.current.onReady({ text: () => view.state.doc.toString(), selection: () => { const range = view.state.selection.main; return view.state.sliceDoc(range.from, range.to); }, insert: text => { view.dispatch(view.state.replaceSelection(text)); view.focus(); }, replace: text => { view.dispatch({ changes: { from: 0, to: view.state.doc.length, insert: text } }); view.focus(); } });
    const loadLanguage = async (): Promise<Extension> => {
      if (props.mode === 'sql') return (await import('@codemirror/lang-sql')).sql();
      if (props.mode === 'cypher') return (await import('./cypher')).cypher();
      return [];
    };
    void loadLanguage().then(extension => { if (!disposed) view.dispatch({ effects: language.reconfigure(extension) }); }).catch(error => { if (!disposed) latest.current.onError(error); });
    return () => { disposed = true; latest.current.onReady(null); view.destroy(); };
  }, [props.mode]);
  return <div class="editor" ref={parent} />;
}
