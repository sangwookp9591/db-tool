import { GlideSelect, type GlideSelectOption, type GlideSelectProps } from './GlideSelect';

export type SelectOption = GlideSelectOption;
export type SelectProps = Omit<GlideSelectProps, 'fitTrigger' | 'showTags'> & { showTags?: boolean; size?: 'sm' | 'md' | 'lg' | 'field' };

/** 입력창과 같은 36px 높이·반경·글꼴을 쓰는 공용 선택 상자. 키보드·typeahead·aria·빈 상태는 GlideSelect가 처리한다.
 *  options 배열은 호출하는 쪽에서 useMemo로 고정해 목록이 매 렌더마다 다시 만들어지지 않게 한다. */
export function Select({ placeholder = '선택…', size = 'field', class: className, className: legacyClass, ...props }: SelectProps) {
  return <GlideSelect {...props} size={size} fitTrigger placeholder={placeholder} class={`wb-select ${className ?? legacyClass ?? ''}`} />;
}
