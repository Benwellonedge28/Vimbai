import { useState, useEffect } from 'react'
import { api } from '../services/api'

export default function Accounts() {
  const [accounts, setAccounts] = useState<any[]>([])
  const [showModal, setShowModal] = useState(false)
  const [loading, setLoading] = useState(true)

  const [newAccount, setNewAccount] = useState({
    name: '', account_number: '', account_type: 'asset', normal_balance: 'debit', description: ''
  })

  useEffect(() => {
    api.getAccounts().then(setAccounts).catch(console.error).finally(() => setLoading(false))
  }, [])

  const handleCreate = async (e: React.FormEvent) => {
    e.preventDefault()
    try {
      const created = await api.createAccount(newAccount)
      setAccounts([...accounts, created])
      setShowModal(false)
      setNewAccount({ name: '', account_number: '', account_type: 'asset', normal_balance: 'debit', description: '' })
    } catch (err) {
      console.error('Failed to create account', err)
    }
  }

  if (loading) return <div className="text-center py-8">Loading...</div>

  return (
    <div>
      <div className="flex justify-between items-center mb-6">
        <h1 className="text-3xl font-bold">Chart of Accounts</h1>
        <button onClick={() => setShowModal(true)} className="bg-indigo-600 text-white px-4 py-2 rounded hover:bg-indigo-700">
          + New Account
        </button>
      </div>

      <div className="bg-white rounded-lg shadow overflow-hidden">
        <table className="min-w-full">
          <thead className="bg-gray-50">
            <tr>
              <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">Account #</th>
              <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">Name</th>
              <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">Type</th>
              <th className="px-4 py-3 text-left text-xs font-medium text-gray-500 uppercase">Normal Balance</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-gray-200">
            {accounts.map((acc) => (
              <tr key={acc.account_number} className="hover:bg-gray-50">
                <td className="px-4 py-3">{acc.account_number}</td>
                <td className="px-4 py-3 font-medium">{acc.name}</td>
                <td className="px-4 py-3">
                  <span className={`px-2 py-1 rounded text-xs ${
                    acc.account_type === 'asset' ? 'bg-green-100 text-green-800' :
                    acc.account_type === 'liability' ? 'bg-red-100 text-red-800' :
                    acc.account_type === 'revenue' ? 'bg-blue-100 text-blue-800' :
                    'bg-gray-100 text-gray-800'
                  }`}>{acc.account_type}</span>
                </td>
                <td className="px-4 py-3">{acc.normal_balance}</td>
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      {showModal && (
        <div className="fixed inset-0 bg-black bg-opacity-50 flex items-center justify-center">
          <div className="bg-white p-6 rounded-lg w-96">
            <h2 className="text-xl font-bold mb-4">Create New Account</h2>
            <form onSubmit={handleCreate}>
              <div className="mb-3">
                <label className="block text-sm mb-1">Account Number</label>
                <input value={newAccount.account_number} onChange={e => setNewAccount({...newAccount, account_number: e.target.value})} className="w-full border p-2 rounded" required />
              </div>
              <div className="mb-3">
                <label className="block text-sm mb-1">Account Name</label>
                <input value={newAccount.name} onChange={e => setNewAccount({...newAccount, name: e.target.value})} className="w-full border p-2 rounded" required />
              </div>
              <div className="mb-3">
                <label className="block text-sm mb-1">Account Type</label>
                <select value={newAccount.account_type} onChange={e => setNewAccount({...newAccount, account_type: e.target.value})} className="w-full border p-2 rounded">
                  <option value="asset">Asset</option>
                  <option value="liability">Liability</option>
                  <option value="equity">Equity</option>
                  <option value="revenue">Revenue</option>
                  <option value="expense">Expense</option>
                </select>
              </div>
              <div className="flex justify-end gap-2 mt-4">
                <button type="button" onClick={() => setShowModal(false)} className="px-4 py-2 border rounded">Cancel</button>
                <button type="submit" className="px-4 py-2 bg-indigo-600 text-white rounded">Create</button>
              </div>
            </form>
          </div>
        </div>
      )}
    </div>
  )
}
