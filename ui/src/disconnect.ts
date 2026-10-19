export function disconnectedTabs<T extends {id:string;profileId:string;text:string;runId:string|null}>(tabs:readonly T[],profileId:string,text:(id:string)=>string|undefined):T[] {
  return tabs.map(tab=>tab.profileId===profileId?{...tab,text:text(tab.id)??tab.text,runId:null}:tab);
}
