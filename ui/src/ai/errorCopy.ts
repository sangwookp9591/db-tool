// 알려진 기술 오류를 차분한 한국어 안내로 바꾼다. 원문은 접이식 상세에 그대로 둔다.
const known:[RegExp,string][]=[
  [/schema context.*(timed? ?out|timeout)|timed? ?out.*schema/i,'스키마 정보를 불러오는 데 시간이 너무 오래 걸렸습니다. 연결 상태를 확인한 뒤 다시 시도해 주세요.'],
  [/timed? ?out|timeout|deadline/i,'응답 시간이 초과되었습니다. 잠시 후 다시 시도해 주세요.'],
  [/rate.?limit|429|too many requests/i,'요청이 너무 많습니다. 잠시 후 다시 시도해 주세요.'],
  [/unauthori[sz]ed|401|invalid.*(api.?key|token)|authentication/i,'인증에 실패했습니다. 계정 연결 또는 API 키를 확인해 주세요.'],
  [/network|fetch failed|connection (refused|reset|closed)|ECONN/i,'네트워크 연결에 문제가 있습니다. 연결을 확인한 뒤 다시 시도해 주세요.'],
];
export function friendlyError(raw:string):{summary:string;detail?:string}{
  const text=raw.trim();
  if(/[가-힣]/.test(text)&&text.length<80)return {summary:text};
  const hit=known.find(([re])=>re.test(text));
  return hit?{summary:hit[1],detail:text}:{summary:'응답을 완료하지 못했습니다. 다시 시도해 주세요.',detail:text};
}
