<script lang="ts">
	/**
	 * A code sample: a caption, a copy button and the code in a scrollable `<pre>`. The pre is
	 * focusable so keyboard users can scroll long lines.
	 */
	import CopyButton from './copy-button.svelte';

	let {
		code,
		caption,
		testid = 'code-block'
	}: { code: string; caption?: string; testid?: string } = $props();
</script>

<figure
	class="flex min-w-0 flex-col overflow-hidden rounded-md border bg-secondary"
	data-testid={testid}
>
	<figcaption class="flex items-center justify-between gap-3 border-b px-3 py-1.5">
		<span class="text-helper text-muted-foreground">{caption ?? ''}</span>
		<CopyButton text={code} variant="ghost" />
	</figcaption>
	<!-- A scrollable region must be focusable (axe: scrollable-region-focusable). -->
	<!-- svelte-ignore a11y_no_noninteractive_tabindex -->
	<pre
		class="overflow-x-auto p-3 font-mono text-cell whitespace-pre"
		tabindex="0"
		aria-label={caption ? `${caption} code` : 'Code'}><code>{code}</code></pre>
</figure>
