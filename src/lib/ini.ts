// Lossless Unreal INI editing – mirrors src-tauri/src/core/ini.rs. Only touched keys change.

type Line = { kind: 'section'; name: string; raw: string } | { kind: 'pair'; key: string; value: string; raw: string } | { kind: 'other'; raw: string };

function parseLine(raw: string): Line {
  const t = raw.trim();
  if (t.startsWith('[') && t.endsWith(']') && t.length >= 2) return { kind: 'section', name: t.slice(1, -1).trim(), raw };
  if (!t || t.startsWith(';') || t.startsWith('#')) return { kind: 'other', raw };
  const eq = t.indexOf('=');
  if (eq > 0) return { kind: 'pair', key: t.slice(0, eq).trim(), value: t.slice(eq + 1), raw };
  return { kind: 'other', raw };
}

export class IniDoc {
  lines: Line[];
  constructor(text: string) {
    const clean = text.replace(/^﻿/, '');
    this.lines = clean ? clean.split('\n').map((l) => parseLine(l.replace(/\r$/, ''))) : [];
    const last = this.lines[this.lines.length - 1];
    if (last && last.kind === 'other' && last.raw === '') this.lines.pop();
  }

  toString() {
    return this.lines.map((l) => l.raw + '\r\n').join('');
  }

  private range(section: string): [number, number] | null {
    const s = this.lines.findIndex((l) => l.kind === 'section' && l.name.toLowerCase() === section.toLowerCase());
    if (s < 0) return null;
    let e = this.lines.findIndex((l, i) => i > s && l.kind === 'section');
    if (e < 0) e = this.lines.length;
    return [s, e];
  }

  get(section: string, key: string): string | undefined {
    const r = this.range(section);
    if (!r) return undefined;
    for (let i = r[1] - 1; i > r[0]; i--) {
      const l = this.lines[i];
      if (l.kind === 'pair' && l.key.toLowerCase() === key.toLowerCase()) return l.value;
    }
    return undefined;
  }

  set(section: string, key: string, value: string) {
    const raw = `${key}=${value}`;
    const r = this.range(section);
    if (!r) {
      if (this.lines.length && this.lines[this.lines.length - 1].raw.trim() !== '') this.lines.push({ kind: 'other', raw: '' });
      this.lines.push({ kind: 'section', name: section, raw: `[${section}]` }, { kind: 'pair', key, value, raw });
      return;
    }
    const idx: number[] = [];
    for (let i = r[0] + 1; i < r[1]; i++) {
      const l = this.lines[i];
      if (l.kind === 'pair' && l.key.toLowerCase() === key.toLowerCase()) idx.push(i);
    }
    if (idx.length) {
      this.lines[idx[idx.length - 1]] = { kind: 'pair', key, value, raw };
      for (const i of idx.slice(0, -1).reverse()) this.lines.splice(i, 1);
    } else {
      let at = r[1];
      while (at > r[0] + 1 && this.lines[at - 1].kind === 'other' && this.lines[at - 1].raw.trim() === '') at--;
      this.lines.splice(at, 0, { kind: 'pair', key, value, raw });
    }
  }

  remove(section: string, key: string) {
    const r = this.range(section);
    if (!r) return;
    const base = (k: string) => k.replace(/^[+\-!.]/, '').toLowerCase();
    for (let i = r[1] - 1; i > r[0]; i--) {
      const l = this.lines[i];
      if (l.kind === 'pair' && base(l.key) === key.toLowerCase()) this.lines.splice(i, 1);
    }
  }
}

export interface IniIssue {
  line: number;
  message: string;
  severity: 'error' | 'warning';
}

/** Light validation for the raw editor. */
export function validateIni(text: string): IniIssue[] {
  const out: IniIssue[] = [];
  let inSection = false;
  text.split('\n').forEach((raw, i) => {
    const t = raw.trim();
    if (!t || t.startsWith(';') || t.startsWith('#')) return;
    if (t.startsWith('[')) {
      if (!t.endsWith(']')) out.push({ line: i + 1, message: 'Section header is missing its closing ]', severity: 'error' });
      inSection = true;
      return;
    }
    if (!t.includes('=')) out.push({ line: i + 1, message: 'Line has no “=” – it will be ignored', severity: 'warning' });
    else if (!inSection) out.push({ line: i + 1, message: 'Setting outside of any [Section] – it will be ignored', severity: 'warning' });
    const v = t.slice(t.indexOf('=') + 1);
    const opens = (v.match(/\(/g) ?? []).length;
    const closes = (v.match(/\)/g) ?? []).length;
    if (opens !== closes) out.push({ line: i + 1, message: 'Unbalanced parentheses in value', severity: 'error' });
    if ((v.match(/"/g) ?? []).length % 2 === 1) out.push({ line: i + 1, message: 'Unclosed quote in value', severity: 'error' });
  });
  return out;
}
