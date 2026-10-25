import { mkdirSync, readdirSync, readFileSync, unlinkSync, writeFileSync } from "node:fs";
import path from "node:path";
import {
  type Action,
  formatAction,
  parseActionText,
  sanitizeActionName,
  tally,
  windowLabel,
} from "./action.ts";

export interface CatalogEntry {
  name: string;
  summary: string;
  window: string;
  readable: boolean;
}

export class ActionStore {
  constructor(readonly dir: string) {
    mkdirSync(dir, { recursive: true });
  }

  pathFor(name: string): string {
    return path.join(this.dir, `${sanitizeActionName(name)}.txt`);
  }

  exists(name: string): boolean {
    const file = this.pathFor(name);
    try {
      readFileSync(file);
      return true;
    } catch {
      return false;
    }
  }

  load(name: string): Action | null {
    try {
      return parseActionText(readFileSync(this.pathFor(name), "utf8"));
    } catch {
      return null;
    }
  }

  readText(name: string): string | null {
    try {
      return readFileSync(this.pathFor(name), "utf8");
    } catch {
      return null;
    }
  }

  save(name: string, action: Action): boolean {
    try {
      writeFileSync(this.pathFor(name), formatAction(action), "utf8");
      return true;
    } catch {
      return false;
    }
  }

  saveText(name: string, text: string): Action | null {
    const action = parseActionText(text);
    if (!action || !this.save(name, action)) return null;
    return action;
  }

  delete(name: string): boolean {
    try {
      unlinkSync(this.pathFor(name));
      return true;
    } catch {
      return false;
    }
  }

  catalog(): CatalogEntry[] {
    let names: string[] = [];
    try {
      names = readdirSync(this.dir)
        .filter((file) => file.endsWith(".txt"))
        .sort();
    } catch {
      return [];
    }
    return names.map((file) => {
      const name = file.slice(0, -4);
      const action = this.load(name);
      if (!action) {
        return { name, summary: "", window: "unreadable file", readable: false };
      }
      return { name, summary: tally(action), window: windowLabel(action), readable: true };
    });
  }
}
