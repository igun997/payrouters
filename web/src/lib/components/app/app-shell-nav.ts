// Navigation for the admin shell: sidebar and icon rail (all items), bottom tab bar (four tabs plus
// More), and the More panel (Products, Webhooks, Playground).
import type { Component } from 'svelte';
import { resolve } from '$app/paths';
import LayoutDashboard from '@lucide/svelte/icons/layout-dashboard';
import CreditCard from '@lucide/svelte/icons/credit-card';
import Banknote from '@lucide/svelte/icons/banknote';
import Package from '@lucide/svelte/icons/package';
import Landmark from '@lucide/svelte/icons/landmark';
import Webhook from '@lucide/svelte/icons/webhook';
import FlaskConical from '@lucide/svelte/icons/flask-conical';
import BookOpen from '@lucide/svelte/icons/book-open';

export type NavItem = { href: string; label: string; icon: Component<{ class?: string }> };

export const overviewHref = resolve('/(admin)');

export const sidebarNav: NavItem[] = [
	{ href: overviewHref, label: 'Overview', icon: LayoutDashboard },
	{ href: resolve('/(admin)/payments'), label: 'Payments', icon: CreditCard },
	{ href: resolve('/(admin)/cash-ins'), label: 'Cash-ins', icon: Banknote },
	{ href: resolve('/(admin)/products'), label: 'Products', icon: Package },
	{ href: resolve('/(admin)/providers'), label: 'Providers', icon: Landmark },
	{ href: resolve('/(admin)/webhooks'), label: 'Webhooks', icon: Webhook },
	{ href: resolve('/(admin)/playground'), label: 'Playground', icon: FlaskConical },
	{ href: resolve('/(admin)/docs'), label: 'API docs', icon: BookOpen }
];

export const tabNav: NavItem[] = [
	{ href: overviewHref, label: 'Overview', icon: LayoutDashboard },
	{ href: resolve('/(admin)/payments'), label: 'Payments', icon: CreditCard },
	{ href: resolve('/(admin)/cash-ins'), label: 'Cash-ins', icon: Banknote },
	{ href: resolve('/(admin)/providers'), label: 'Providers', icon: Landmark }
];

/** Overview matches only the root; other items match their path and sub-paths. */
export function isActive(path: string, href: string): boolean {
	if (href === overviewHref) return path === href;
	return path === href || path.startsWith(`${href}/`);
}

/** Fallback page titles for routes whose pages do not publish a title to the shell. */
export function fallbackTitle(path: string): string {
	const match = sidebarNav.find((item) => item.href !== overviewHref && isActive(path, item.href));
	if (match) return match.label;
	return path === overviewHref ? 'Overview' : 'PayRouters';
}
