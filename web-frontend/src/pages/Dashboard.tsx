import { useState, useEffect } from 'react'
import { api } from '../services/api'

export default function Dashboard() {
  const [accounts, setAccounts] = useState<any[]>([])
  const [trialBalance, setTrialBalance] = useState<any>(null)
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    const fetchData = async () => {
      try {
        const [accData, tbData] = await Promise.all([
          api.getAccounts(),
          api.getTrialBalance()
        ])
        setAccounts(accData)
        setTrialBalance(tbData)
      } catch (err) {
        console.error('Failed to load dashboard data', err)
      } finally {
        setLoading(false)
      }
    }
    fetchData()
  }, [])

  if (loading) return <div className="text-center py-8">Loading...</div>

  return (
    <div>
      <h1 className="text-3xl font-bold mb-6">Financial Dashboard</h1>
      
      <div className="grid grid-cols-1 md:grid-cols-3 gap-6 mb-8">
        <div className="bg-white p-6 rounded-lg shadow">
          <h3 className="text-gray-500 text-sm">Total Accounts</h3>
          <p className="text-3xl font-bold">{accounts.length}</p>
        </div>
        <div className="bg-white p-6 rounded-lg shadow">
          <h3 className="text-gray-500 text-sm">Total Debits</h3>
          <p className="text-3xl font-bold text-green-600">
            ${trialBalance?.total_debits?.toLocaleString() || '0'}
          </p>
        </div>
        <div className="bg-white p-6 rounded-lg shadow">
          <h3 className="text-gray-500 text-sm">Total Credits</h3>
          <p className="text-3xl font-bold text-blue-600">
            ${trialBalance?.total_credits?.toLocaleString() || '0'}
          </p>
        </div>
      </div>

      <div className="bg-white p-6 rounded-lg shadow">
        <h2 className="text-xl font-bold mb-4">Trial Balance</h2>
        {trialBalance && (
          <div className="overflow-x-auto">
            <table className="min-w-full">
              <thead className="bg-gray-50">
                <tr>
                  <th className="px-4 py-2 text-left">Account</th>
                  <th className="px-4 py-2 text-right">Debit</th>
                  <th className="px-4 py-2 text-right">Credit</th>
                </tr>
              </thead>
              <tbody>
                {trialBalance.accounts?.map((acc: any) => (
                  <tr key={acc.account_number} className="border-t">
                    <td className="px-4 py-2">{acc.account_name}</td>
                    <td className="px-4 py-2 text-right">${acc.debit?.toLocaleString() || '0'}</td>
                    <td className="px-4 py-2 text-right">${acc.credit?.toLocaleString() || '0'}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </div>
  )
}
