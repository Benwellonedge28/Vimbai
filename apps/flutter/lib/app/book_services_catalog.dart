// Book services catalog: which services correspond to each Book type.
//
// Every Book tier (personal, household, group, business, nonprofit)
// has its own set of services. The home screen reads this catalog to
// show exactly the services that fit the active Book.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/features/banking/presentation/bank_accounts_page.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/balance_sheet_page.dart';
import 'package:vimbai_mobile_client/features/budgeting/presentation/budgets_page.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/cash_flow_statement_page.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/currency_exposure_page.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/exchange_differences_page.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/fx_restatement_page.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/chart_of_accounts_page.dart';
import 'package:vimbai_mobile_client/features/reporting/presentation/financial_ratios_page.dart';
import 'package:vimbai_mobile_client/features/group_savings/presentation/group_savings_page.dart';
import 'package:vimbai_mobile_client/features/household_finance/presentation/household_finance_page.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/income_statement_page.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/journal_entries_list_page.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/ledger_page.dart';
import 'package:vimbai_mobile_client/features/invoicing/presentation/discounts_page.dart';
import 'package:vimbai_mobile_client/features/depreciation/presentation/post_depreciation_page.dart';
import 'package:vimbai_mobile_client/features/expenses/presentation/record_expense_page.dart';
import 'package:vimbai_mobile_client/features/invoicing/presentation/invoices_page.dart';
import 'package:vimbai_mobile_client/features/invoicing/presentation/return_from_customer_page.dart';
import 'package:vimbai_mobile_client/features/invoicing/presentation/customer_register_page.dart';
import 'package:vimbai_mobile_client/features/assets/presentation/fixed_assets_page.dart';
import 'package:vimbai_mobile_client/features/budgeting/presentation/spending_plan_page.dart';
import 'package:vimbai_mobile_client/features/payables/presentation/buy_stock_page.dart';
import 'package:vimbai_mobile_client/features/payables/presentation/due_plan_page.dart';
import 'package:vimbai_mobile_client/features/payables/presentation/payables_page.dart';
import 'package:vimbai_mobile_client/features/payables/presentation/return_to_supplier_page.dart';
import 'package:vimbai_mobile_client/features/payroll/presentation/payroll_page.dart';
import 'package:vimbai_mobile_client/features/cashbook/presentation/cash_book_page.dart';
import 'package:vimbai_mobile_client/features/npo/presentation/npo_page.dart';
import 'package:vimbai_mobile_client/features/personal_finance/presentation/personal_finance_page.dart';
import 'package:vimbai_mobile_client/features/overview/presentation/where_you_stand_page.dart';
import 'package:vimbai_mobile_client/features/close/presentation/books_in_order_page.dart';
import 'package:vimbai_mobile_client/features/close/presentation/close_the_month_page.dart';
import 'package:vimbai_mobile_client/features/tax/presentation/what_you_owe_page.dart';
import 'package:vimbai_mobile_client/features/banking/presentation/currencies_page.dart';
import 'package:vimbai_mobile_client/features/tax/presentation/tax_table_page.dart';
import 'package:vimbai_mobile_client/features/trend/presentation/year_trend_page.dart';
import 'package:vimbai_mobile_client/features/integrity/presentation/ledger_integrity_page.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/trial_balance_page.dart';

/// One service entry available to a Book type.
class BookServiceEntry {
  final String title;
  final String description;
  final IconData icon;
  final WidgetBuilder page;

  BookServiceEntry({
    required this.title,
    required this.description,
    required this.icon,
    required this.page,
  });
}

const Map<String, String> kTierLabels = {
  'personal': 'Personal Book',
  'household': 'Household Book',
  'group': 'Group Book',
  'business': 'Business Book',
  'nonprofit': 'Non-profit Book',
};

/// The services that correspond to a Book tier.
List<BookServiceEntry> servicesForTier(String tier) {
  switch (tier) {
    case 'personal':
      return [
        BookServiceEntry(
          title: 'Personal finance',
          description:
              'Recurring bills and income, debts, investments and tax estimation',
          icon: Icons.person_outline,
          page: (context) => const PersonalFinancePage(),
        ),
        BookServiceEntry(
          title: 'Budgets',
          description: 'Budget planning with variance analysis',
          icon: Icons.savings_outlined,
          page: (context) => const BudgetsPage(),
        ),
        BookServiceEntry(
          title: 'Bank accounts',
          description: 'Connected accounts and balances',
          icon: Icons.account_balance_outlined,
          page: (context) => const BankAccountsPage(),
        ),
      ];
    case 'household':
      return [
        BookServiceEntry(
          title: 'Household finance',
          description:
              'Analyze household income, expenses, assets and liabilities',
          icon: Icons.home_outlined,
          page: (context) => const HouseholdFinancePage(),
        ),
        BookServiceEntry(
          title: 'Budgets',
          description: 'Household budget planning with variance analysis',
          icon: Icons.savings_outlined,
          page: (context) => const BudgetsPage(),
        ),
        BookServiceEntry(
          title: 'Bank accounts',
          description: 'Connected accounts and balances',
          icon: Icons.account_balance_outlined,
          page: (context) => const BankAccountsPage(),
        ),
      ];
    case 'group':
      return [
        BookServiceEntry(
          title: 'Group savings',
          description:
              'Savings groups: members, contribution cycles and payouts',
          icon: Icons.groups_outlined,
          page: (context) => const GroupSavingsPage(),
        ),
        BookServiceEntry(
          title: 'Budgets',
          description: 'Shared budget planning with variance analysis',
          icon: Icons.savings_outlined,
          page: (context) => const BudgetsPage(),
        ),
      ];
    case 'nonprofit':
      return [
        BookServiceEntry(
          title: 'Non-profit organizations',
          description:
              'NPO profiles, size bands, donations and donor-grade reports',
          icon: Icons.volunteer_activism_outlined,
          page: (context) => const NpoPage(),
        ),
        BookServiceEntry(
          title: 'Balance sheet',
          description: 'Statement of financial position',
          icon: Icons.balance_outlined,
          page: (context) => const BalanceSheetPage(),
        ),
        BookServiceEntry(
          title: 'Income statement',
          description: 'Statement of activities',
          icon: Icons.receipt_long_outlined,
          page: (context) => const IncomeStatementPage(),
        ),
        BookServiceEntry(
          title: 'Budgets',
          description: 'Programme budget planning with variance analysis',
          icon: Icons.savings_outlined,
          page: (context) => const BudgetsPage(),
        ),
        BookServiceEntry(
          title: 'Bank accounts',
          description: 'Connected accounts and balances',
          icon: Icons.account_balance_outlined,
          page: (context) => const BankAccountsPage(),
        ),
      ];
    case 'business':
    default:
      return [
        BookServiceEntry(
          title: 'Proof nothing has changed',
          description:
              'That every posted entry is still what it was when it was written',
          icon: Icons.verified_outlined,
          page: (context) => const LedgerIntegrityPage(),
        ),
        BookServiceEntry(
          title: 'How the year has gone',
          description:
              'Each month so far: what came in, what went out, what stayed',
          icon: Icons.timeline_outlined,
          page: (context) => const YearTrendPage(),
        ),
        BookServiceEntry(
          title: 'Books in order',
          description:
              'Whether the ledger adds up, what is still in draft, and whether suppliers agree',
          icon: Icons.fact_check_outlined,
          page: (context) => const BooksInOrderPage(),
        ),
        BookServiceEntry(
          title: 'What you owe the taxman',
          description:
              'Tax charged on sales, tax reclaimed on purchases, and what is due',
          icon: Icons.receipt_long_outlined,
          page: (context) => const WhatYouOwePage(),
        ),
        BookServiceEntry(
          title: 'The currencies you keep',
          description:
              'The money this Book deals in, and the decimal places each currency has',
          icon: Icons.currency_exchange_outlined,
          page: (context) => const CurrenciesPage(),
        ),
        BookServiceEntry(
          title: 'The taxes you charge',
          description:
              'Each tax, its rate and account, and the day it starts applying',
          icon: Icons.percent_outlined,
          page: (context) => const TaxTablePage(),
        ),
        BookServiceEntry(
          title: 'The discounts you give',
          description:
              'Each discount, what it takes off, who it is for, and whether it is given automatically',
          icon: Icons.local_offer_outlined,
          page: (context) => const DiscountsPage(),
        ),
        BookServiceEntry(
          title: 'Close the month',
          description: 'Make a month final, so nothing more is posted into it',
          icon: Icons.lock_clock_outlined,
          page: (context) => const CloseTheMonthPage(),
        ),
        BookServiceEntry(
          title: 'Where you stand',
          description:
              'Money held, owed to you, owed by you, and what you have made',
          icon: Icons.insights,
          page: (context) => const WhereYouStandPage(),
        ),
        BookServiceEntry(
          title: 'Everyone you bill',
          description:
              'Your customers, how to reach them, and what each one owes',
          icon: Icons.people_outline,
          page: (context) => const CustomerRegisterPage(),
        ),
        BookServiceEntry(
          title: 'Sales invoices',
          description: 'Bill a customer, and see what is still to come in',
          icon: Icons.receipt_long,
          page: (context) => const InvoicesPage(),
        ),
        BookServiceEntry(
          title: 'What you owe',
          description:
              'Suppliers owed money, bills received, and what you have paid',
          icon: Icons.receipt,
          page: (context) => const PayablesPage(),
        ),
        BookServiceEntry(
          title: 'When you must pay',
          description:
              'What is late, what falls due this week, and what can wait',
          icon: Icons.schedule,
          page: (context) => const DuePlanPage(),
        ),
        BookServiceEntry(
          title: 'What you planned',
          description:
              'What each account may take up, and what it actually took up',
          icon: Icons.savings_outlined,
          page: (context) => const SpendingPlanPage(),
        ),
        BookServiceEntry(
          title: 'Things you own',
          description:
              'Vehicles, machines and equipment, and what is left of what they cost',
          icon: Icons.inventory_2_outlined,
          page: (context) => const FixedAssetsPage(),
        ),
        BookServiceEntry(
          title: 'Payroll',
          description:
              'A month of pay: what each person takes home, and what it costs',
          icon: Icons.groups_outlined,
          page: (context) => const PayrollPage(),
        ),
        BookServiceEntry(
          title: 'Cash book',
          description: 'What the business holds, and every receipt and payment',
          icon: Icons.account_balance_wallet_outlined,
          page: (context) => const CashBookPage(),
        ),
        BookServiceEntry(
          title: 'Chart of accounts',
          description: 'Your business account structure',
          icon: Icons.account_tree_outlined,
          page: (context) => const ChartOfAccountsPage(),
        ),
        BookServiceEntry(
          title: 'Journal entries',
          description: 'Record and review double-entry transactions',
          icon: Icons.edit_note,
          page: (context) => const JournalEntriesListPage(),
        ),
        BookServiceEntry(
          title: 'Ledger',
          description: 'Account-by-account transaction history',
          icon: Icons.menu_book_outlined,
          page: (context) => const LedgerPage(),
        ),
        BookServiceEntry(
          title: 'Trial balance',
          description: 'Period-end debit and credit totals',
          icon: Icons.scale_outlined,
          page: (context) => const TrialBalancePage(),
        ),
        BookServiceEntry(
          title: 'Balance sheet',
          description: 'Financial position statement',
          icon: Icons.balance_outlined,
          page: (context) => const BalanceSheetPage(),
        ),
        BookServiceEntry(
          title: 'Income statement',
          description: 'Profit and loss statement',
          icon: Icons.receipt_long_outlined,
          page: (context) => const IncomeStatementPage(),
        ),
        BookServiceEntry(
          title: 'Cash flow statement',
          description: 'Where the money went, and what for',
          icon: Icons.currency_exchange,
          page: (context) => const CashFlowStatementPage(),
        ),
        BookServiceEntry(
          title: 'Restate the month',
          description:
              'Foreign balances at the closing rate, and the difference',
          icon: Icons.sync_alt,
          page: (context) => const FxRestatementPage(),
        ),
        BookServiceEntry(
          title: 'Stock bought on account',
          description: 'A credit purchase, into inventory and payables',
          icon: Icons.inventory_2_outlined,
          page: (context) => const BuyStockPage(),
        ),
        BookServiceEntry(
          title: 'An expense',
          description: 'Rent, fuel, wages: paid now or still owed',
          icon: Icons.receipt_outlined,
          page: (context) => const RecordExpensePage(),
        ),
        BookServiceEntry(
          title: 'Wear on what you own',
          description: 'A month of use on your assets, into the books',
          icon: Icons.trending_down,
          page: (context) => const PostDepreciationPage(),
        ),
        BookServiceEntry(
          title: 'Goods back from a customer',
          description: 'A return, taking the sale back and what they owe down',
          icon: Icons.keyboard_return_outlined,
          page: (context) => const ReturnFromCustomerPage(),
        ),
        BookServiceEntry(
          title: 'Goods back to a supplier',
          description: 'A return, taking the cost back out of what you owe',
          icon: Icons.assignment_return_outlined,
          page: (context) => const ReturnToSupplierPage(),
        ),
        BookServiceEntry(
          title: 'What you hold in each money',
          description:
              'Still owed to you, and still owed by you, money by money',
          icon: Icons.account_balance_wallet_outlined,
          page: (context) => const CurrencyExposurePage(),
        ),
        BookServiceEntry(
          title: 'What the rate did',
          description: 'Gains and losses from trading in another money',
          icon: Icons.currency_exchange,
          page: (context) => const ExchangeDifferencesPage(),
        ),
        BookServiceEntry(
          title: 'Financial ratios',
          description: 'Liquidity, solvency and profitability ratios',
          icon: Icons.insights_outlined,
          page: (context) => const FinancialRatiosPage(),
        ),
        BookServiceEntry(
          title: 'Budgets',
          description: 'Budget planning with variance analysis',
          icon: Icons.savings_outlined,
          page: (context) => const BudgetsPage(),
        ),
        BookServiceEntry(
          title: 'Bank accounts',
          description: 'Connected accounts and balances',
          icon: Icons.account_balance_outlined,
          page: (context) => const BankAccountsPage(),
        ),
      ];
  }
}
