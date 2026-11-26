<script lang="ts">
	/** Inline outcome of "Test connection", kept under the account until the next test. */
	import Banner from '#lib/components/app/banner.svelte';

	let { result }: { result: { ok: boolean; message: string } | null } = $props();
</script>

{#if result}
	<div data-testid="test-result" data-ok={result.ok}>
		<Banner tone={result.ok ? 'success' : 'error'}>{result.message}</Banner>
	</div>
{/if}
