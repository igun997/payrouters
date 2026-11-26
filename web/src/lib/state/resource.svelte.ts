/**
 * Reactive async-load state for one piece of data. `load` ignores stale responses, so a
 * slow earlier request can never overwrite the result of a newer one.
 */
export class Resource<T> {
	data = $state<T | undefined>(undefined);
	error = $state<unknown>(null);
	loading = $state(false);
	#seq = 0;

	async load(fetcher: () => Promise<T>): Promise<void> {
		const seq = ++this.#seq;
		this.loading = true;
		this.error = null;
		try {
			const data = await fetcher();
			if (seq === this.#seq) this.data = data;
		} catch (error) {
			if (seq === this.#seq) this.error = error;
		} finally {
			if (seq === this.#seq) this.loading = false;
		}
	}

	/** Replace the data locally (e.g. after a mutation returned the new object). */
	set(data: T): void {
		this.data = data;
	}
}
