import type { Value } from './contract';
export function parseDocument(raw: string): Value {
  const tokens=raw.match(/"(?:\\.|[^"\\])*"|-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?|true|false|null|[{}\[\],:]|\S/g) ?? [];
  let i=0;
  function parse(depth: number): Value {
    // shortcut: nesting beyond 128 stays as raw JSON, use an iterative parser if deeper trees are needed.
    if (depth>128) throw new Error('Document nesting exceeds 128');
    const token=tokens[i++];
    if (token==='{' || token==='[') {
      const map:[string,Value][]=[],list:Value[]=[],end=token==='{'?'}':']';
      if(tokens[i]!==end) do {
        if(token==='{') {const key=tokens[i++];if(!key?.startsWith('"')||tokens[i++]!==':')throw new Error('Invalid JSON object');map.push([JSON.parse(key) as string,parse(depth+1)]);}else list.push(parse(depth+1));
        if(tokens[i]===end)break;
        if(tokens[i++]!==',')throw new Error('Invalid JSON separator');
      }while(true);
      if(tokens[i++]!==end)throw new Error('Invalid JSON container');
      return token==='{'?{t:'Map',v:map}:{t:'List',v:list};
    }
    if(token==='null')return {t:'Null'};
    if(token==='true'||token==='false')return {t:'Bool',v:token==='true'};
    if(token?.startsWith('"'))return {t:'Text',v:JSON.parse(token) as string};
    if(token && /^-?(?:0|[1-9]\d*)(?:\.\d+)?(?:[eE][+-]?\d+)?$/.test(token))return {t:/[.eE]/.test(token)?'Decimal':'Int',v:token};
    throw new Error('Invalid JSON value');
  }
  const result=parse(0);if(i!==tokens.length)throw new Error('Trailing JSON content');return result;
}
const parsed=new WeakMap<Value,Value>();
export function children(value: Value): [string,Value][] {
  if(value.t==='Json') {let p=parsed.get(value);if(!p){try{p=parseDocument(value.v);}catch{return [];}parsed.set(value,p);}return children(p);}
  if(value.t==='Map')return value.v;
  if(value.t==='List')return value.v.map((v,i)=>[`[${i}]`,v]);
  if(value.t==='Node'||value.t==='Rel')return value.v.props;
  return [];
}
export function fieldPath(parent: string,key: string,index=false): string {
  if(index && /^\[\d+\]$/.test(key))return parent+key;
  return /^[A-Za-z_$][\w$]*$/.test(key)?`${parent}${parent?'.':''}${key}`:`${parent}[${JSON.stringify(key)}]`;
}
export interface DocumentRow { id: string; path: string; column: string; name: string; value: Value; depth: number; expandable: boolean }
export function documentRows(values: Value[],open: Set<string>): DocumentRow[] {
  const rows:DocumentRow[]=[];
  function visit(value: Value,id: string,path: string,column: string,name: string,depth: number) {
    const expandable=value.t==='Json' ? /^[\s]*[\[{]/.test(value.v) : value.t==='Map'||value.t==='List' ? value.v.length>0 : value.t==='Node'||value.t==='Rel' ? value.v.props.length>0 : false;
    rows.push({id,path,column,name,value,depth,expandable});
    if(open.has(id)&&depth<128)children(value).forEach(([key,v],i)=>visit(v,`${id}/${i}`,fieldPath(path,key,value.t==='List'||parsed.get(value)?.t==='List'),column||key,key,depth+1));
  }
  values.forEach((v,i)=>visit(v,String(i),'','',`Document ${i+1}`,0));return rows;
}
