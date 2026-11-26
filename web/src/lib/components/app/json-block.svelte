<script lang="ts">
	/**
	 * Pretty-printed JSON (or a raw string), one line per element inside a `<pre>`. Long values wrap
	 * within the box; the box scrolls when the document is taller than `max-h-96`.
	 */
	let { value }: { value: unknown } = $props();

	const text = $derived.by(() => {
		if (typeof value === 'string') return value;
		return JSON.stringify(value, null, 2) ?? 'null';
	});
</script>

<pre
	data-testid="json-block"
	class="max-h-96 overflow-auto rounded-md border bg-secondary p-3 font-mono text-cell break-words whitespace-pre-wrap">{text}</pre>
