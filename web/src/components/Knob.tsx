import { useRef } from 'react'
import type { KeyboardEvent, PointerEvent } from 'react'

const DRAG_PX = 200 // vertical pixels for the full range

export function Knob(p: {
  label: string
  value: number
  min: number
  max: number
  resetValue: number
  format(v: number): string
  disabled?: boolean
  onChange(v: number): void
}) {
  const { label, value, min, max, resetValue, format, disabled, onChange } = p
  const range = max - min
  // Accumulates unclamped drag position so Shift can toggle mid-drag without a jump.
  const drag = useRef<{ y: number; v: number } | null>(null)
  const set = (v: number) => onChange(Math.min(max, Math.max(min, v)))

  function onPointerDown(e: PointerEvent<HTMLDivElement>) {
    if (disabled || e.button !== 0) return
    e.currentTarget.setPointerCapture(e.pointerId)
    drag.current = { y: e.clientY, v: value }
  }
  function onPointerMove(e: PointerEvent<HTMLDivElement>) {
    const d = drag.current
    if (!d) return
    const v = d.v + ((d.y - e.clientY) / DRAG_PX) * range * (e.shiftKey ? 0.1 : 1)
    drag.current = { y: e.clientY, v: Math.min(max, Math.max(min, v)) }
    set(v)
  }
  function onPointerUp() {
    drag.current = null
  }
  function onKeyDown(e: KeyboardEvent<HTMLDivElement>) {
    if (disabled) return
    const step = {
      ArrowUp: 0.01, ArrowRight: 0.01, ArrowDown: -0.01, ArrowLeft: -0.01, PageUp: 0.1, PageDown: -0.1,
    }[e.key]
    if (step !== undefined) set(value + step * range)
    else if (e.key === 'Home') set(min)
    else if (e.key === 'End') set(max)
    else return
    e.preventDefault()
  }

  const angle = -135 + (270 * (value - min)) / range
  return (
    <div className={'knob' + (disabled ? ' disabled' : '')}>
      <div
        className="knob-dial"
        role="slider"
        tabIndex={disabled ? -1 : 0}
        aria-label={label}
        aria-valuemin={min}
        aria-valuemax={max}
        aria-valuenow={value}
        aria-valuetext={format(value)}
        aria-disabled={disabled || undefined}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerUp}
        onPointerCancel={onPointerUp}
        onKeyDown={onKeyDown}
        onDoubleClick={() => !disabled && set(resetValue)}
      >
        <span className="knob-cap" style={{ transform: `rotate(${angle}deg)` }} />
      </div>
      <span className="knob-label">{label}</span>
      <span className="knob-value">{format(value)}</span>
    </div>
  )
}
