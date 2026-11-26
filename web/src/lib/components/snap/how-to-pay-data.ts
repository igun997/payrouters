// "How to pay" steps per bank and per Mandiri bill payment. Shown as a disclosure list on the Snap page.

export interface Guide {
	title: string;
	steps: string[];
}

const VA_GUIDES: Record<string, Guide[]> = {
	bca: [
		{
			title: 'm-BCA mobile banking',
			steps: [
				'Open m-BCA and choose m-Transfer, then BCA Virtual Account.',
				'Enter the account number above and confirm the amount.',
				'Check the account name, then enter your PIN to pay.'
			]
		},
		{
			title: 'ATM BCA',
			steps: [
				'Insert your BCA ATM card and enter your PIN.',
				'Choose Transaksi Lainnya, then Transfer, then BCA Virtual Account.',
				'Enter the account number above, check the amount and name, then confirm.'
			]
		},
		{
			title: 'Klik BCA internet banking',
			steps: [
				'Log in to KlikBCA and choose Transfer Dana.',
				'Choose BCA Virtual Account and enter the account number above.',
				'Check the amount and name, then confirm with your KeyBCA token.'
			]
		}
	],
	bni: [
		{
			title: 'BNI Mobile Banking',
			steps: [
				'Open BNI Mobile Banking and choose Transfer, then Virtual Account Billing.',
				'Enter the account number above and confirm the amount.',
				'Check the account name, then enter your PIN to pay.'
			]
		},
		{
			title: 'ATM BNI',
			steps: [
				'Insert your BNI ATM card and enter your PIN.',
				'Choose Transfer, then Virtual Account Billing.',
				'Enter the account number above, check the amount, then confirm.'
			]
		}
	],
	bri: [
		{
			title: 'BRImo',
			steps: [
				'Open BRImo and choose Pembayaran, then BRIVA.',
				'Enter the account number above and confirm the amount.',
				'Check the account name, then enter your PIN to pay.'
			]
		},
		{
			title: 'ATM BRI',
			steps: [
				'Insert your BRI ATM card and enter your PIN.',
				'Choose Transaksi Lain, then Pembayaran, then BRIVA.',
				'Enter the account number above and confirm.'
			]
		}
	],
	permata: [
		{
			title: 'PermataMobile X',
			steps: [
				'Open PermataMobile X and choose Pembayaran, then Virtual Account.',
				'Enter the account number above and confirm the amount.',
				'Check the account name, then enter your PIN to pay.'
			]
		},
		{
			title: 'ATM Permata',
			steps: [
				'Insert your Permata ATM card and enter your PIN.',
				'Choose Transaksi Lainnya, then Pembayaran, then Virtual Account.',
				'Enter the account number above and confirm.'
			]
		}
	]
};

const FALLBACK_VA: Guide[] = [
	{
		title: 'Mobile or internet banking',
		steps: [
			'Open your bank app and choose Transfer, then Virtual Account.',
			'Enter the account number above and confirm the amount.',
			'Check the account name, then enter your PIN to pay.'
		]
	}
];

export function vaGuides(bank: string): Guide[] {
	return VA_GUIDES[bank.toLowerCase()] ?? FALLBACK_VA;
}

/** `total` is the formatted amount the customer must pay, e.g. "Rp 150.000". */
export function mandiriBillGuides(total: string): Guide[] {
	return [
		{
			title: 'm-Mandiri (Bayar > Multipayment)',
			steps: [
				'Open m-Mandiri and choose Bayar, then Multipayment.',
				'Enter the biller code and the bill key above.',
				`Check the amount is ${total}, then enter your PIN.`
			]
		},
		{
			title: "Livin' by Mandiri",
			steps: [
				"Open Livin' by Mandiri and choose Bayar, then Multipayment.",
				'Enter the biller code and the bill key above.',
				`Check the amount is ${total}, then confirm with your PIN.`
			]
		}
	];
}
