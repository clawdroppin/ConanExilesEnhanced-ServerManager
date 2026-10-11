/** Tag colours a server can wear (tab stripe, sidebar dot, cards). */
export const SERVER_COLORS: Record<string, { label: string; hex: string }> = {
  bronze: { label: 'Bronze', hex: '#f59e0b' },
  ember: { label: 'Ember', hex: '#f97316' },
  blood: { label: 'Blood', hex: '#ef4444' },
  gold: { label: 'Gold', hex: '#fbbf24' },
  emerald: { label: 'Emerald', hex: '#34d399' },
  sky: { label: 'Sky', hex: '#38bdf8' },
  violet: { label: 'Violet', hex: '#a78bfa' },
  rose: { label: 'Rose', hex: '#fb7185' },
  stone: { label: 'Stone', hex: '#a8a29e' },
};

export function serverColor(c: string | undefined) {
  return SERVER_COLORS[c ?? 'bronze']?.hex ?? SERVER_COLORS.bronze.hex;
}
