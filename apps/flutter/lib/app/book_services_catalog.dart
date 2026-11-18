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
import 'package:vimbai_mobile_client/features/accounting/presentation/chart_of_accounts_page.dart';
import 'package:vimbai_mobile_client/features/reporting/presentation/financial_ratios_page.dart';
import 'package:vimbai_mobile_client/features/group_savings/presentation/group_savings_page.dart';
import 'package:vimbai_mobile_client/features/household_finance/presentation/household_finance_page.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/income_statement_page.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/journal_entries_list_page.dart';
import 'package:vimbai_mobile_client/features/accounting/presentation/ledger_page.dart';
import 'package:vimbai_mobile_client/features/invoicing/presentation/invoices_page.dart';
import 'package:vimbai_mobile_client/features/invoicing/presentation/customer_register_page.dart';
import 'package:vimbai_mobile_client/features/payables/presentation/payables_page.dart';
import 'package:vimbai_mobile_client/features/payroll/presentation/payroll_page.dart';
import 'package:vimbai_mobile_client/features/cashbook/presentation/cash_book_page.dart';
import 'package:vimbai_mobile_client/features/npo/presentation/npo_page.dart';
import 'package:vimbai_mobile_client/features/personal_finance/presentation/personal_finance_page.dart';
import 'package:vimbai_mobile_client/features/overview/presentation/where_you_stand_page.dart';
import 'package:vimbai_mobile_client/features/close/presentation/books_in_order_page.dart';
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
