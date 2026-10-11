import { useEffect, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { Command, Copy, Minus, Square, X } from 'lucide-react';
import { inTauri } from '@/lib/ipc';
import { useApp } from '@/store/app';
import { Kbd } from '@/components/ui/Surface';

function Logo() {
  return (
    <svg viewBox="0 0 1024 1024" className="size-[18px]" aria-hidden>
      <defs>
        <linearGradient id="tb-gold" x1="0.5" y1="1" x2="0.5" y2="0">
          <stop offset="0" stopColor="#b45309" />
          <stop offset="0.6" stopColor="#f59e0b" />
          <stop offset="1" stopColor="#fcd34d" />
        </linearGradient>
      </defs>
      {/* crossed swords behind the shield – blades up, hilts down */}
      <g strokeLinecap="round">
        <line x1="330" y1="240" x2="694" y2="604" stroke="#d6d3d1" strokeWidth="44" />
        <line x1="694" y1="240" x2="330" y2="604" stroke="#d6d3d1" strokeWidth="44" />
        <line x1="330" y1="240" x2="430" y2="340" stroke="#f5f5f4" strokeWidth="44" />
        <line x1="694" y1="240" x2="594" y2="340" stroke="#f5f5f4" strokeWidth="44" />
        <line x1="452" y1="582" x2="694" y2="824" stroke="#92400e" strokeWidth="52" />
        <line x1="572" y1="582" x2="330" y2="824" stroke="#92400e" strokeWidth="52" />
        <line x1="420" y1="640" x2="520" y2="540" stroke="url(#tb-gold)" strokeWidth="40" />
        <line x1="604" y1="640" x2="504" y2="540" stroke="url(#tb-gold)" strokeWidth="40" />
        <circle cx="706" cy="836" r="34" fill="url(#tb-gold)" />
        <circle cx="318" cy="836" r="34" fill="url(#tb-gold)" />
      </g>
      {/* shield */}
      <path
        d="M512 268 L706 330 V520 C706 648 622 716 512 776 C402 716 318 648 318 520 V330 Z"
        fill="#221a12"
        stroke="url(#tb-gold)"
        strokeWidth="34"
        strokeLinejoin="round"
      />
      {/* the C */}
      <path
        d="M596 436 A118 118 0 1 0 596 588"
        fill="none"
        stroke="url(#tb-gold)"
        strokeWidth="56"
        strokeLinecap="round"
      />
    </svg>
  );
}

export function TitleBar() {
  const [max, setMax] = useState(false);
  const setPalette = useApp((s) => s.setPalette);
  const win = inTauri() ? getCurrentWindow() : null;
  useEffect(() => {
    if (!win) return;
    win.isMaximized().then(setMax);
    const un = win.onResized(() => win.isMaximized().then(setMax));
    return () => {
      un.then((f) => f());
    };
  }, [win]);

  return (
    <div data-tauri-drag-region className="relative z-30 flex h-10 shrink-0 items-center border-b border-line bg-bg pl-3.5">
      <div data-tauri-drag-region className="flex items-center gap-2.5">
        <Logo />
        <span data-tauri-drag-region className="text-[12.5px] font-semibold tracking-tight text-fg">
          Conan Exiles <span className="text-fg-3">Server Manager</span>
        </span>
      </div>
      <div data-tauri-drag-region className="flex flex-1 justify-center">
        <button
          onClick={() => setPalette(true)}
          className="flex h-7 w-[340px] items-center gap-2 rounded-lg border border-line bg-panel px-2.5 text-[12px] text-fg-4 transition-colors hover:border-line-strong hover:text-fg-3"
        >
          <Command className="size-3.5" />
          Search servers, actions, settings…
          <span className="ml-auto flex gap-0.5">
            <Kbd>Ctrl</Kbd>
            <Kbd>K</Kbd>
          </span>
        </button>
      </div>
      {win && (
        <div className="flex h-full">
          <button onClick={() => win.minimize()} className="flex h-full w-11 items-center justify-center text-fg-3 hover:bg-hover hover:text-fg">
            <Minus className="size-4" />
          </button>
          <button onClick={() => win.toggleMaximize()} className="flex h-full w-11 items-center justify-center text-fg-3 hover:bg-hover hover:text-fg">
            {max ? <Copy className="size-3.5 -scale-x-100" /> : <Square className="size-3.5" />}
          </button>
          <button onClick={() => win.close()} className="flex h-full w-11 items-center justify-center text-fg-3 hover:bg-err hover:text-white">
            <X className="size-4" />
          </button>
        </div>
      )}
    </div>
  );
}
