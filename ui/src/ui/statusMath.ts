export type MarkStatus='pending'|'running'|'done'|'failed'|'cancelled';
export function markGeometry(mode:number,arc:number,circumference:number,dashes=8){
 const period=circumference/Math.max(1,dashes);
 return {dash:Math.max(0,.3*period+(arc*circumference-.3*period)*mode),gap:Math.max(0,.7*period+((1-arc)*circumference-.7*period)*mode)};
}
