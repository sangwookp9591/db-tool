export const pulseSwell=(t:number,overshoot=1.7)=>{
 if(t<=0||t>=1)return 0;
 if(t<.4)return 1-(1-t/.4)**3;
 const u=(t-.4)/.6-1;
 return -((overshoot+1)*u**3+overshoot*u**2);
};
