import { useState, useEffect } from 'react'
import { api } from '../services/api'

interface BankAccount {
  id: string
  bankName: string
  accountName: string
  accountType: string
  currentBalance: number
  currency: string
  isSynced: boolean
  lastSyncedAt: string | null
}

interface BankTransaction {
  id: string
  bankAccountId: string
  date: string
  description: string
  amount: number
  transactionType: string
  reconciled: boolean
}

export default function Banking() {
  const [accounts, setAccounts] = useState<BankAccount[]>([])
  const [transactions, setTransactions] = useState<BankTransaction[]>([])
  const [loading, setLoading] = useState(true)
  const [selectedAccount, setSelectedAccount] = useState<string | null>(null)

  useEffect(() => {
    fetchBankAccounts()
  }, [])

  const fetchBankAccounts = async () => {
    try {
      setLoading(true)
      const data = await api.get<BankAccount[]>('/banking/accounts')
      setAccounts(data)
      if (data.length > 0) {
        setSelectedAccount(data[0].id)
        fetchTransactions(data[0].id)
      }
    } catch (error) {
      console.error('Failed to fetch bank accounts:', error)
    } finally {
      setLoading(false)
    }
  }

  const fetchTransactions = async (accountId: string) => {
    try {
      const data = await api.get<BankTransaction[]>(`/banking/accounts/${accountId}/transactions`)
      setTransactions(data)
    } catch (error) {
      console.error('Failed to fetch transactions:', error)
    }
  }

  const handleReconcile = async (transactionId: string) => {
    try {
      await api.post(`/banking/transactions/${transactionId}/reconcile`, {})
      fetchTransactions(selectedAccount!)
    } catch (error) {
      console.error('Failed to reconcile transaction:', error)
    }
  }

  const handleSyncAccount = async (accountId: string) => {
    try {
      await api.post(`/banking/accounts/${accountId}/sync`, {})
      fetchBankAccounts()
    } catch (error) {
      console.error('Failed to sync account:', error)
    }
  }

  if (loading) {
    return (
      <div className="flex items-center justify-center h-64">
        <div className="animate-spin rounded-full h-8 w-8 border-b-2 border-indigo-600"></div>
      </div>
    )
  }

  return (
    <div className="space-y-6">
      <div className="flex justify-between items-center">
        <h1 className="text-2xl font-bold text-gray-900">Banking Integration</h1>
        <button
          onClick={fetchBankAccounts}
          className="px-4 py-2 bg-indigo-600 text-white rounded-lg hover:bg-indigo-700"
        >
          Refresh
        </button>
      </div>

      {/* Bank Accounts */}
      <div className="bg-white shadow rounded-lg p-6">
        <h2 className="text-lg font-semibold mb-4">Connected Bank Accounts</h2>
        <div className="grid grid-cols-1 md:grid-cols-2 lg:grid-cols-3 gap-4">
          {accounts.map((account) => (
            <div
              key={account.id}
              onClick={() => {
                setSelectedAccount(account.id)
                fetchTransactions(account.id)
              }}
              className={`p-4 border rounded-lg cursor-pointer transition ${
                selectedAccount === account.id
                  ? 'border-indigo-500 bg-indigo-50'
                  : 'border-gray-200 hover:border-indigo-300'
              }`}
            >
              <div className="flex justify-between items-start mb-2">
                <h3 className="font-semibold">{account.bankName}</h3>
                <span className={`text-xs px-2 py-1 rounded ${
                  account.isSynced ? 'bg-green-100 text-green-800' : 'bg-yellow-100 text-yellow-800'
                }`}>
                  {account.isSynced ? 'Synced' : 'Pending'}
                </span>
              </div>
              <p className="text-sm text-gray-600">{account.accountName}</p>
              <p className="text-lg font-bold mt-2">
                {account.currency} {account.currentBalance.toLocaleString()}
              </p>
              <div className="flex justify-between items-center mt-3">
                <span className="text-xs text-gray-500">
                  {account.accountType}
                </span>
                <button
                  onClick={(e) => {
                    e.stopPropagation()
                    handleSyncAccount(account.id)
                  }}
                  className="text-xs text-indigo-600 hover:text-indigo-800"
                >
                  Sync Now
                </button>
              </div>
            </div>
          ))}
        </div>
        {accounts.length === 0 && (
          <p className="text-gray-500 text-center py-8">No bank accounts connected yet.</p>
        )}
      </div>

      {/* Transactions */}
      {selectedAccount && (
        <div className="bg-white shadow rounded-lg p-6">
          <h2 className="text-lg font-semibold mb-4">Recent Transactions</h2>
          <div className="overflow-x-auto">
            <table className="min-w-full divide-y divide-gray-200">
              <thead className="bg-gray-50">
                <tr>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">Date</th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">Description</th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">Amount</th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">Type</th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">Status</th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">Action</th>
                </tr>
              </thead>
              <tbody className="divide-y divide-gray-200">
                {transactions.map((transaction) => (
                  <tr key={transaction.id} className="hover:bg-gray-50">
                    <td className="px-6 py-4 whitespace-nowrap text-sm">{transaction.date}</td>
                    <td className="px-6 py-4 text-sm">{transaction.description}</td>
                    <td className={`px-6 py-4 whitespace-nowrap text-sm font-medium ${
                      transaction.transactionType === 'credit' ? 'text-green-600' : 'text-red-600'
                    }`}>
                      {transaction.transactionType === 'credit' ? '+' : '-'}${Math.abs(transaction.amount).toLocaleString()}
                    </td>
                    <td className="px-6 py-4 whitespace-nowrap text-sm capitalize">{transaction.transactionType}</td>
                    <td className="px-6 py-4 whitespace-nowrap">
                      <span className={`px-2 py-1 text-xs rounded ${
                        transaction.reconciled ? 'bg-green-100 text-green-800' : 'bg-yellow-100 text-yellow-800'
                      }`}>
                        {transaction.reconciled ? 'Reconciled' : 'Pending'}
                      </span>
                    </td>
                    <td className="px-6 py-4 whitespace-nowrap">
                      {!transaction.reconciled && (
                        <button
                          onClick={() => handleReconcile(transaction.id)}
                          className="text-sm text-indigo-600 hover:text-indigo-800"
                        >
                          Reconcile
                        </button>
                      )}
                    </td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
          {transactions.length === 0 && (
            <p className="text-gray-500 text-center py-8">No transactions found.</p>
          )}
        </div>
      )}
    </div>
  )
}