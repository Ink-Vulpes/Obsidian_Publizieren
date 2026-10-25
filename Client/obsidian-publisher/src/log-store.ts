export interface LogEntry {
	timestamp: number;
	push: string[];
	pull: string[];
	deleteRemote: string[];
	deleteLocal: string[];
	errors: string[];
}

const MAX_ENTRIES = 200;

export class LogStore {
	private entries: LogEntry[] = [];
	private listeners: Array<() => void> = [];

	load(entries: LogEntry[]) {
		this.entries = entries ?? [];
	}

	getEntries(): LogEntry[] {
		return [...this.entries].reverse(); // newest first
	}

	add(entry: LogEntry) {
		this.entries.push(entry);
		if (this.entries.length > MAX_ENTRIES) {
			this.entries = this.entries.slice(-MAX_ENTRIES);
		}
		this.notify();
	}

	clear() {
		this.entries = [];
		this.notify();
	}

	serialize(): LogEntry[] {
		return [...this.entries];
	}

	onChange(fn: () => void): () => void {
		this.listeners.push(fn);
		return () => { this.listeners = this.listeners.filter(l => l !== fn); };
	}

	private notify() {
		for (const fn of this.listeners) fn();
	}
}
