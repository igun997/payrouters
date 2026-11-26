import type { Page } from '#lib/api/types';

/**
 * Cursor-paged list for "Load more" lists. `reset` loads the first page and drops what was shown;
 * `more` appends the next page. A response from a superseded `reset` is ignored, so a slow request
 * can never land on top of a newer filter's rows.
 */
export class PagedList<T> {
	items = $state<T[]>([]);
	nextCursor = $state<string | null>(null);
	loading = $state(false);
	loadingMore = $state(false);
	error = $state<unknown>(null);
	#generation = 0;

	async reset(fetchPage: () => Promise<Page<T>>): Promise<void> {
		const generation = ++this.#generation;
		this.loading = true;
		this.loadingMore = false;
		this.error = null;
		try {
			const page = await fetchPage();
			if (generation !== this.#generation) return;
			this.items = page.data;
			this.nextCursor = page.next_cursor;
		} catch (error) {
			if (generation !== this.#generation) return;
			this.items = [];
			this.nextCursor = null;
			this.error = error;
		} finally {
			if (generation === this.#generation) this.loading = false;
		}
	}

	async more(fetchPage: (cursor: string) => Promise<Page<T>>): Promise<void> {
		const cursor = this.nextCursor;
		if (!cursor || this.loadingMore) return;
		const generation = this.#generation;
		this.loadingMore = true;
		try {
			const page = await fetchPage(cursor);
			if (generation !== this.#generation) return;
			this.items = [...this.items, ...page.data];
			this.nextCursor = page.next_cursor;
		} catch (error) {
			if (generation === this.#generation) this.error = error;
		} finally {
			if (generation === this.#generation) this.loadingMore = false;
		}
	}

	/** Changes rows in place (e.g. a retried delivery returned as the new object). */
	update(transform: (items: T[]) => T[]): void {
		this.items = transform(this.items);
	}
}
