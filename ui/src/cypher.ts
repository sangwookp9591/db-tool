import { StreamLanguage } from '@codemirror/language';
export const cypher = () => StreamLanguage.define({
  startState: () => ({ quote: '' }),
  token(stream, state) {
    if (state.quote) { let escaped = false; let char; while ((char = stream.next()) !== undefined) { if (char === state.quote && !escaped) { state.quote = ''; break; } escaped = char === '\\' && !escaped; } return 'string'; }
    if (stream.eatSpace()) return null;
    if (stream.match('//')) { stream.skipToEnd(); return 'comment'; }
    if (stream.match(/['"`]/)) { state.quote = stream.current(); return 'string'; }
    if (stream.match(/\b(?:MATCH|OPTIONAL|WHERE|RETURN|WITH|CREATE|MERGE|DELETE|DETACH|SET|REMOVE|UNWIND|CALL|YIELD|ORDER|BY|LIMIT|SKIP|DISTINCT|AS|AND|OR|NOT|IN|IS|NULL|TRUE|FALSE|ASC|DESC|UNION)\b/i)) return 'keyword';
    if (stream.match(/\$[\w]+/)) return 'variableName';
    if (stream.match(/\d+(?:\.\d+)?/)) return 'number';
    stream.next(); return null;
  }
});
