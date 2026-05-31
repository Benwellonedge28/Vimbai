import { useState } from 'react'
import { api } from '../services/api'

export default function Reports() {
  const [reportType, setReportType] = useState('trial_balance')
  const [startDate, setStartDate] = useState('')
  const [endDate, setEndDate] = useState('')
  const [reportData, setReportData] = useState<any>(null)
  const [loading, setLoading] = useState(false)

  const generateReport = async () => {
    setLoading(true)
    try {
      let result
      switch (reportType) {
        case 'trial_balance':
          result = await api.getTrialBalance(startDate)
          break
        case 'income_statement':
          result = await api.getIncomeStatement(startDate, endDate)
          break
        case 'balance_sheet':
          result = await api.getBalanceSheet(endDate)
          break
      }
      setReportData(result)
    } catch (err) {
      console.error('Failed to generate report', err)
    } finally {
      setLoading(false)
    }
  }

  return (
    <div>
      <h1 className="text-3xl font-bold mb-6">Financial Reports</h1>

      <div className="bg-white p-6 rounded-lg shadow mb-6">
        <div className="flex flex-wrap gap-4 items-end">
          <div>
            <label className="block text-sm mb-1">Report Type</label>
            <select value={reportType} onChange={e => setReportType(e.target.value)} className="border p-2 rounded">
              <option value="trial_balance">Trial Balance</option>
              <option value="income_statement">Income Statement</option>
              <option value="balance_sheet">Balance Sheet</option>
            </select>
          </div>
          <div>
            <label className="block text-sm mb-1">Start Date</label>
            <input type="date" value={startDate} onChange={e => setStartDate(e.target.value)} className="border p-2 rounded" />
          </div>
          <div>
            <label className="block text-sm mb-1">End Date</label>
            <input type="date" value={endDate} onChange={e => setEndDate(e.target.value)} className="border p-2 rounded" />
          </div>
          <button onClick={generateReport} disabled={loading} className="bg-indigo-600 text-white px-4 py-2 rounded hover:bg-indigo-700 disabled:opacity-50">
            {loading ? 'Generating...' : 'Generate Report'}
          </button>
        </div>
      </div>

      {reportData && (
        <div className="bg-white p-6 rounded-lg shadow">
          <h2 className="text-xl font-bold mb-4 capitalize">{reportType.replace('_', ' ')}</h2>
          <pre className="bg-gray-100 p-4 rounded overflow-x-auto">
            {JSON.stringify(reportData, null, 2)}
          </pre>
        </div>
      )}
    </div>
  )
}
