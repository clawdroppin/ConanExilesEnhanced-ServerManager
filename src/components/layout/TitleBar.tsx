import { useEffect, useState } from 'react';
import { getCurrentWindow } from '@tauri-apps/api/window';
import { Command, Copy, Minus, Square, X } from 'lucide-react';
import { inTauri } from '@/lib/ipc';
import { useApp } from '@/store/app';
import { Kbd } from '@/components/ui/Surface';

function Logo() {
  return (
    <svg viewBox="0 0 1024 1024" className="size-[18px]">
      <defs>
        <linearGradient id="tb-fire" x1="0.5" y1="1" x2="0.5" y2="0">
          <stop offset="0" stopColor="#ea580c" />
          <stop offset="0.55" stopColor="#f97316" />
          <stop offset="1" stopColor="#fcd34d" />
        </linearGradient>
      </defs>
      <path
        d="M512 150c40 120 170 190 190 340 14 104-36 190-110 232 26-60 18-128-30-178-6 70-46 118-92 140 18-58 4-110-38-152-20 76-78 116-78 196 0 34 10 66 28 92-90-40-148-128-148-230 0-170 160-250 210-330 20 60 6 110-14 150 70-40 104-140 82-260z"
        fill="url(#tb-fire)"
      />
      <rect x="300" y="770" width="424" height="56" rx="18" fill="#fafaf9" />
      <rect x="340" y="846" width="344" height="44" rx="16" fill="#fafaf9" opacity="0.6" />
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
