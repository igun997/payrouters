// In-page layout detector for the long-content spec. One call inspects the current DOM and returns
// plain-text findings, so a failing expectation prints every defect with its selector.
//
// Findings:
//   page-scroll   the document is wider than its client area (any horizontal scroll of the page)
//   offscreen     a visible box, after clipping by its overflow ancestors, extends past the viewport
//                 edges; boxes inside a horizontal scroller (overflow-x auto|scroll) are excused
//   text-overflow an element with its own text has scrollWidth > clientWidth and does not scroll;
//                 clipping (overflow hidden + text-overflow ellipsis) is allowed only when the full
//                 text is exposed through a title or aria-label
//   text-overlap  two text boxes, neither containing the other, share more than 4 px² of area;
//                 fixed and absolute elements are skipped (overlays by design), and so are pairs
//                 that sit in different fixed layers (a dialog over the page behind it)
import type { Page } from '@playwright/test';

/** A box in viewport pixels, after clipping by overflow ancestors. */
export interface Box {
	left: number;
	top: number;
	right: number;
	bottom: number;
	width: number;
	height: number;
}

/** Returns one line per defect: `kind | selector | detail`. Empty when the layout is clean. */
export async function layoutIssues(page: Page): Promise<string[]> {
	return page.evaluate(() => {
		const vw = window.innerWidth;
		const out: string[] = [];

		const describe = (el: Element): string => {
			const h = el as HTMLElement;
			let sel = h.tagName.toLowerCase();
			if (h.dataset.testid) sel += `[data-testid="${h.dataset.testid}"]`;
			else if (h.id) sel += `#${h.id}`;
			const text = (h.innerText || h.textContent || '').trim().replace(/\s+/g, ' ').slice(0, 60);
			return `${sel} "${text}"`;
		};

		// Shown means no display:none / visibility:hidden / opacity:0 on the element or an ancestor,
		// and a non-empty box.
		const isShown = (el: HTMLElement): boolean => {
			let child: Element = el;
			for (let n: Element | null = el; n; child = n, n = n.parentElement) {
				const s = getComputedStyle(n);
				if (s.display === 'none' || s.visibility === 'hidden' || Number(s.opacity) === 0) {
					return false;
				}
				// A closed <details> shows only its <summary>; its body is laid out but not seen.
				if (
					n.tagName === 'DETAILS' &&
					!(n as HTMLDetailsElement).open &&
					child.tagName !== 'SUMMARY'
				) {
					return false;
				}
			}
			const r = el.getBoundingClientRect();
			return r.width > 0 && r.height > 0;
		};

		// The element's box intersected with every overflow-clipping ancestor (what is visible).
		const clippedBox = (el: HTMLElement): Box => {
			const r = el.getBoundingClientRect();
			let left = r.left;
			let top = r.top;
			let right = r.right;
			let bottom = r.bottom;
			for (let p = el.parentElement; p && p !== document.body; p = p.parentElement) {
				const s = getComputedStyle(p);
				if (s.overflowX === 'visible' && s.overflowY === 'visible') continue;
				const pr = p.getBoundingClientRect();
				left = Math.max(left, pr.left);
				top = Math.max(top, pr.top);
				right = Math.min(right, pr.right);
				bottom = Math.min(bottom, pr.bottom);
			}
			return {
				left,
				top,
				right,
				bottom,
				width: Math.max(0, right - left),
				height: Math.max(0, bottom - top)
			};
		};

		const inScroller = (el: HTMLElement): boolean => {
			for (let p = el.parentElement; p; p = p.parentElement) {
				const ox = getComputedStyle(p).overflowX;
				if (ox === 'auto' || ox === 'scroll') return true;
			}
			return false;
		};

		// The nearest fixed or sticky ancestor (or self): the layer an element is painted in. Sticky bars
		// (the Snap status line, the payment action bar) overlay the page by design.
		const fixedLayer = (el: HTMLElement): Element | null => {
			for (let n: Element | null = el; n && n !== document.body; n = n.parentElement) {
				const p = getComputedStyle(n).position;
				if (p === 'fixed' || p === 'sticky') return n;
			}
			return null;
		};

		const texts: { el: HTMLElement; box: Box; layer: Element | null }[] = [];

		for (const el of document.body.querySelectorAll<HTMLElement>('*')) {
			if (el instanceof SVGElement) continue;
			if (!isShown(el)) continue;
			const s = getComputedStyle(el);
			const box = clippedBox(el);
			const hasText = [...el.childNodes].some(
				(n) => n.nodeType === Node.TEXT_NODE && (n.textContent ?? '').trim() !== ''
			);

			// offscreen: the visible part of the box runs past a viewport edge.
			if (box.width > 0 && (box.left < -1 || box.right > vw + 1) && !inScroller(el)) {
				out.push(
					`offscreen | ${describe(el)} | visible box ${Math.round(box.left)}..${Math.round(box.right)} in viewport 0..${vw}`
				);
			}

			// sr-only and other 1px clips hold text for screen readers, not for sight.
			if (
				!hasText ||
				el.getBoundingClientRect().width <= 1 ||
				el.getBoundingClientRect().height <= 1
			) {
				continue;
			}

			// text-overflow: own text wider than the box and not scrollable.
			if (s.display !== 'inline' && s.display !== 'contents') {
				const ox = s.overflowX;
				if (el.scrollWidth > el.clientWidth + 1 && ox !== 'auto' && ox !== 'scroll') {
					const clips = ox === 'hidden' || ox === 'clip';
					const ellipsis = clips && s.textOverflow === 'ellipsis';
					const exposed = !!(el.title || el.getAttribute('aria-label'));
					if (!(ellipsis && exposed)) {
						const why = ellipsis
							? 'ellipsis without the full text in title or aria-label'
							: clips
								? 'clipped without ellipsis'
								: `overflow-x ${ox}`;
						out.push(
							`text-overflow | ${describe(el)} | scrollWidth ${el.scrollWidth} > clientWidth ${el.clientWidth} (${why})`
						);
					}
				}
			}

			// text-overflow, inline and flex-item text: the text's extent runs past the nearest block box
			// that contains it. Covers spans and other text-only children that scrollWidth cannot see.
			const selfClips = s.overflowX === 'hidden' || s.overflowX === 'clip';
			if (!selfClips && s.position !== 'absolute' && s.position !== 'fixed' && !inScroller(el)) {
				let container: HTMLElement | null = el.parentElement;
				while (
					container &&
					container !== document.body &&
					['inline', 'contents'].includes(getComputedStyle(container).display)
				) {
					container = container.parentElement;
				}
				if (container && container !== document.body) {
					const cs = getComputedStyle(container);
					const cr = container.getBoundingClientRect();
					const left = cr.left + container.clientLeft;
					const right = left + container.clientWidth;
					let textLeft = Infinity;
					let textRight = -Infinity;
					for (const n of el.childNodes) {
						if (n.nodeType !== Node.TEXT_NODE || (n.textContent ?? '').trim() === '') continue;
						const range = document.createRange();
						range.selectNodeContents(n);
						const r = range.getBoundingClientRect();
						if (r.width === 0) continue;
						textLeft = Math.min(textLeft, r.left);
						textRight = Math.max(textRight, r.right);
					}
					if (textRight > right + 1 || textLeft < left - 1) {
						const ox = cs.overflowX;
						const exposed = !!(el.title || el.getAttribute('aria-label') || container.title);
						const clips = ox === 'hidden' || ox === 'clip';
						const ellipsis = cs.textOverflow === 'ellipsis' || s.textOverflow === 'ellipsis';
						if (ox !== 'auto' && ox !== 'scroll' && !(clips && ellipsis && exposed)) {
							const why = clips ? 'clipped by its box' : `container overflow-x ${ox}`;
							out.push(
								`text-overflow | ${describe(el)} | text ${Math.round(textLeft)}..${Math.round(textRight)} past ${container.tagName.toLowerCase()} ${Math.round(left)}..${Math.round(right)} (${why})`
							);
						}
					}
				}
			}

			if (
				s.position !== 'sticky' &&
				s.position !== 'fixed' &&
				s.position !== 'absolute' &&
				box.width >= 2 &&
				box.height >= 2
			) {
				texts.push({ el, box, layer: fixedLayer(el) });
			}
		}

		// text-overlap: pairwise over text boxes in the same layer, excluding ancestor pairs.
		for (let i = 0; i < texts.length; i++) {
			for (let j = i + 1; j < texts.length; j++) {
				const a = texts[i];
				const b = texts[j];
				if (a.layer !== b.layer) continue;
				if (a.el.contains(b.el) || b.el.contains(a.el)) continue;
				const w = Math.min(a.box.right, b.box.right) - Math.max(a.box.left, b.box.left);
				const h = Math.min(a.box.bottom, b.box.bottom) - Math.max(a.box.top, b.box.top);
				if (w > 0 && h > 0 && w * h > 4) {
					out.push(
						`text-overlap | ${describe(a.el)} | ${describe(b.el)} | ${Math.round(w * h)} px² shared`
					);
				}
			}
		}

		const docWidth = document.documentElement.scrollWidth;
		const clientWidth = document.documentElement.clientWidth;
		if (docWidth > clientWidth + 1) {
			out.unshift(`page-scroll | html | scrollWidth ${docWidth} > clientWidth ${clientWidth}`);
		}

		return [...new Set(out)];
	});
}
