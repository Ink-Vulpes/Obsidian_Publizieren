export type DiffLine = { type: 'equal' | 'insert' | 'delete'; text: string };

const CONTEXT = 3;
const MAX_LINES = 600;

export function lineDiff(oldText: string, newText: string): DiffLine[] {
	const old = oldText.split('\n').slice(0, MAX_LINES);
	const neu = newText.split('\n').slice(0, MAX_LINES);
	const m = old.length;
	const n = neu.length;

	// ── LCS DP table (flat Uint32Array for speed) ──────────────────────────
	const dp = new Uint32Array((m + 1) * (n + 1));
	const W = n + 1;
	for (let i = 1; i <= m; i++) {
		for (let j = 1; j <= n; j++) {
			dp[i * W + j] = old[i - 1] === neu[j - 1]
				? dp[(i - 1) * W + (j - 1)] + 1
				: Math.max(dp[(i - 1) * W + j], dp[i * W + (j - 1)]);
		}
	}

	// ── iterative backtrack ────────────────────────────────────────────────
	const full: DiffLine[] = [];
	let i = m, j = n;
	while (i > 0 || j > 0) {
		if (i > 0 && j > 0 && old[i - 1] === neu[j - 1]) {
			full.unshift({ type: 'equal', text: old[i - 1] });
			i--; j--;
		} else if (j > 0 && (i === 0 || dp[i * W + (j - 1)] >= dp[(i - 1) * W + j])) {
			full.unshift({ type: 'insert', text: neu[j - 1] });
			j--;
		} else {
			full.unshift({ type: 'delete', text: old[i - 1] });
			i--;
		}
	}
	return full;
}

/** Collapse long equal runs, keeping CONTEXT lines around each change. */
export function withContext(lines: DiffLine[]): Array<DiffLine | { type: 'gap'; count: number }> {
	type Out = DiffLine | { type: 'gap'; count: number };
	if (lines.every(l => l.type === 'equal')) return lines;

	const result: Out[] = [];
	let i = 0;
	while (i < lines.length) {
		const line = lines[i];
		if (line.type !== 'equal') {
			result.push(line);
			i++;
			continue;
		}
		// Find the run of equal lines
		let runEnd = i;
		while (runEnd < lines.length && lines[runEnd].type === 'equal') runEnd++;
		const runLen = runEnd - i;

		const prevChanged = i > 0 && lines[i - 1].type !== 'equal';
		const nextChanged = runEnd < lines.length;

		if (!prevChanged && !nextChanged) {
			// All equal — only show first/last context
			if (runLen <= CONTEXT * 2 + 1) {
				for (let k = i; k < runEnd; k++) result.push(lines[k]);
			} else {
				for (let k = i; k < i + CONTEXT; k++) result.push(lines[k]);
				result.push({ type: 'gap', count: runLen - CONTEXT * 2 });
				for (let k = runEnd - CONTEXT; k < runEnd; k++) result.push(lines[k]);
			}
		} else if (!prevChanged) {
			// Leading equal run before a change
			if (runLen <= CONTEXT) {
				for (let k = i; k < runEnd; k++) result.push(lines[k]);
			} else {
				result.push({ type: 'gap', count: runLen - CONTEXT });
				for (let k = runEnd - CONTEXT; k < runEnd; k++) result.push(lines[k]);
			}
		} else if (!nextChanged) {
			// Trailing equal run after a change
			const show = Math.min(CONTEXT, runLen);
			for (let k = i; k < i + show; k++) result.push(lines[k]);
			if (runLen > show) result.push({ type: 'gap', count: runLen - show });
		} else {
			// Between two changed sections
			if (runLen <= CONTEXT * 2) {
				for (let k = i; k < runEnd; k++) result.push(lines[k]);
			} else {
				for (let k = i; k < i + CONTEXT; k++) result.push(lines[k]);
				result.push({ type: 'gap', count: runLen - CONTEXT * 2 });
				for (let k = runEnd - CONTEXT; k < runEnd; k++) result.push(lines[k]);
			}
		}
		i = runEnd;
	}
	return result;
}

export function isTextFile(path: string): boolean {
	return /\.(md|txt|json|yaml|yml|toml|csv|js|ts|css|html|xml|svg|canvas)$/i.test(path);
}
