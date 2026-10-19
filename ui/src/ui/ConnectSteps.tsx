import type { ComponentChildren } from 'preact';
import { Button, type ButtonProps } from './Button';
import { StatusMark } from './StatusMark';
import type { MarkStatus } from './statusMath';
import './ConnectSteps.css';

export interface ConnectStep {
  id: string;
  label: string;
  status: MarkStatus;
  elapsedMs?: number;
  ariaLabel?: string;
}
export function ConnectSteps({ steps, label }: { steps: ConnectStep[]; label: string }) {
  return <ol class="connect-steps ai-connect-steps" aria-label={label}>{steps.map(step =>
    <li key={step.id} data-status={step.status} data-active={step.status === 'running' || step.status === 'done'} aria-current={step.status === 'running' ? 'step' : undefined}>
      <span aria-label={step.ariaLabel}><StatusMark status={step.status} label={step.label} strike={false} /></span>
      {step.elapsedMs != null && <small class="ax-test-time">{step.elapsedMs} ms</small>}
    </li>
  )}</ol>;
}
export function ConnectButton({ state, children = '연결', ...props }: Omit<ButtonProps, 'loading'> & { state: 'idle' | 'running' | 'done' | 'failed'; children?: ComponentChildren }) {
  return <Button morph={false} {...props} loading={state === 'running'}>{state === 'done' ? '연결됨 ✓' : state === 'running' ? '연결 중…' : children}</Button>;
}
