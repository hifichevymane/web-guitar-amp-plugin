/** Input peak (linear 0..1) shown over -60..0 dB; red above -3 dB. */
export function LevelMeter({ peak }: { peak: number }) {
  const db = peak > 0 ? 20 * Math.log10(peak) : -Infinity
  const width = Math.min(100, Math.max(0, ((db + 60) / 60) * 100))
  return (
    <div className="meter" role="meter" aria-label="Input level" aria-valuemin={-60} aria-valuemax={0}
      aria-valuenow={Math.max(-60, Math.min(0, Math.round(db)))}>
      <b className={db > -3 ? 'hot' : undefined} style={{ width: `${width}%` }} />
    </div>
  )
}
