// Book services catalog: which services correspond to each Book type.
//
// Every Book tier (personal, household, group, business, nonprofit)
// has its own set of services. The home screen reads this catalog to
// show exactly the services that fit the active Book.
import 'package:flutter/material.dart';

import 'package:vimbai_mobile_client/pages/bank_accounts_page.dart';
import 'package:vimbai_mobile_client/pages/balance_sheet_page.dart';
import 'package:vimbai_mobile_client/pages/budgets_page.dart';
import 'package:vimbai_mobile_client/pages/cash_flow_statement_page.dart';
import 'package:vimbai_mobile_client/pages/chart_of_accounts_page.dart';
import 'package:vimbai_mobile_client/pages/financial_ratios_page.dart';
import 'package:vimbai_mobile_client/pages/group_savings_page.dart';
import 'package:vimbai_mobile_client/pages/household_finance_page.dart';
import 'package:vimbai_mobile_client/pages/income_statement_page.dart';
import 'package:vimbai_mobile_client/pages/journal_entries_list_page.dart';
import 'package:vimbai_mobile_client/pages/ledger_page.dart';
import 'package:vimbai_mobile_client/pages/npo_page.dart';
import 'package:vimbai_mobile_client/pages/personal_finance_page.dart';
import 'package:vimbai_mobile_client/pages/trial_balance_page.dart';

/// One service entry available to a Book type.
class BookServiceEntry {
  final String title;
  final String description;
  final IconData icon;
  final WidgetBuilder page;

  const BookServiceEntry({
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
      return const [
        BookServiceEntry(
          title: 'Personal finance',
          description: 'Recurring bills and income, debts, investments and tax estimation',
          icon: Icons.person_outline,
          page: PersonalFinancePage.new,
        ),
        BookServiceEntry(
          title: 'Budgets',
          description: 'Budget planning with variance analysis',
          icon: Icons.savings_outlined,
          page: BudgetsPage.new,
        ),
        BookServiceEntry(
          title: 'Bank accounts',
          description: 'Connected accounts and balances',
          icon: Icons.account_balance_outlined,
          page: BankAccountsPage.new,
        ),
      ];
    case 'household':
      return const [
        BookServiceEntry(
          title: 'Household finance',
          description: 'Analyze household income, expenses, assets and liabilities',
          icon: Icons.home_outlined,
          page: HouseholdFinancePage.new,
        ),
        BookServiceEntry(
          title: 'Budgets',
          description: 'Household budget planning with variance analysis',
          icon: Icons.savings_outlined,
          page: BudgetsPage.new,
        ),
        BookServiceEntry(
          title: 'Bank accounts',
          description: 'Connected accounts and balances',
          icon: Icons.account_balance_outlined,
          page: BankAccountsPage.new,
        ),
      ];
    case 'group':
      return const [
        BookServiceEntry(
          title: 'Group savings',
          description: 'Savings groups: members, contribution cycles and payouts',
          icon: Icons.groups_outlined,
          page: GroupSavingsPage.new,
        ),
        BookServiceEntry(
          title: 'Budgets',
          description: 'Shared budget planning with variance analysis',
          icon: Icons.savings_outlined,
          page: BudgetsPage.new,
        ),
      ];
    case 'nonprofit':
      return const [
        BookServiceEntry(
          title: 'Non-profit organizations',
          description: 'NPO profiles, size bands, donations and donor-grade reports',
          icon: Icons.volunteer_activism_outlined,
          page: NpoPage.new,
        ),
        BookServiceEntry(
          title: 'Balance sheet',
          description: 'Statement of financial position',
          icon: Icons.balance_outlined,
          page: BalanceSheetPage.new,
        ),
        BookServiceEntry(
          title: 'Income statement',
          description: 'Statement of activities',
          icon: Icons.receipt_long_outlined,
          page: IncomeStatementPage.new,
        ),
        BookServiceEntry(
          title: 'Budgets',
          description: 'Programme budget planning with variance analysis',
          icon: Icons.savings_outlined,
          page: BudgetsPage.new,
        ),
        BookServiceEntry(
          title: 'Bank accounts',
          description: 'Connected accounts and balances',
          icon: Icons.account_balance_outlined,
          page: BankAccountsPage.new,
        ),
      ];
    case 'business':
    default:
      return const [
        BookServiceEntry(
          title: 'Chart of accounts',
          description: 'Your business account structure',
          icon: Icons.account_tree_outlined,
          page: ChartOfAccountsPage.new,
        ),
        BookServiceEntry(
          title: 'Journal entries',
          description: 'Record and review double-entry transactions',
          icon: Icons.edit_note,
          page: JournalEntriesListPage.new,
        ),
        BookServiceEntry(
          title: 'Ledger',
          description: 'Account-by-account transaction history',
          icon: Icons.menu_book_outlined,
          page: LedgerPage.new,
        ),
        BookServiceEntry(
          title: 'Trial balance',
          description: 'Period-end debit and credit totals',
          icon: Icons.scale_outlined,
          page: TrialBalancePage.new,
        ),
        BookServiceEntry(
          title: 'Balance sheet',
          description: 'Financial position statement',
          icon: Icons.balance_outlined,
          page: BalanceSheetPage.new,
        ),
        BookServiceEntry(
          title: 'Income statement',
          description: 'Profit and loss statement',
          icon: Icons.receipt_long_outlined,
          page: IncomeStatementPage.new,
        ),
        BookServiceEntry(
          title: 'Cash flow statement',
          description: 'Operating, investing and financing flows',
          icon: Icons.currency_exchange,
          page: CashFlowStatementPage.new,
        ),
        BookServiceEntry(
          title: 'Financial ratios',
          description: 'Liquidity, solvency and profitability ratios',
          icon: Icons.insights_outlined,
          page: FinancialRatiosPage.new,
        ),
        BookServiceEntry(
          title: 'Budgets',
          description: 'Budget planning with variance analysis',
          icon: Icons.savings_outlined,
          page: BudgetsPage.new,
        ),
        BookServiceEntry(
          title: 'Bank accounts',
          description: 'Connected accounts and balances',
          icon: Icons.account_balance_outlined,
          page: BankAccountsPage.new,
        ),
      ];
  }
}
