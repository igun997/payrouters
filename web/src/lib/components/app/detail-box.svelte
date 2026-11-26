<script lang="ts">
	/** Key/value summary inside a dialog (what is being revoked, deleted or blocked). */
	let {
		rows
	}: {
		rows: { label: string; value: string; mono?: boolean }[];
	} = $props();
</script>

<dl class="flex flex-col divide-y rounded-md border bg-card" data-testid="detail-box">
	{#each rows as row (row.label)}
		<div class="flex items-center justify-between gap-4 px-3.5 py-2.5">
			<dt class="text-helper text-muted-foreground">{row.label}</dt>
			<dd
				class="min-w-0 truncate text-right text-body text-foreground {row.mono
					? 'text-mono-id font-mono'
					: ''}"
			>
				{row.value}
			</dd>
		</div>
	{/each}
</dl>
