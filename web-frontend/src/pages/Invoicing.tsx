import { useState, useEffect } from 'react'
import { api } from '../services/api'

interface Invoice {
  id: string
  invoiceNumber: string
  customerId: string
  customerName: string
  date: string
  dueDate: string
  total: number
  status: 'draft' | 'sent' | 'paid' | 'overdue' | 'cancelled'
  currency: string
}

export default function Invoicing() {
  const [invoices, setInvoices] = useState<Invoice[]>([])
  const [loading, setLoading] = useState(true)
  const [filter, setFilter] = useState<string>('all')

  useEffect(() => {
    fetchInvoices()
  }, [filter])

  const fetchInvoices = async () => {
    try {
      setLoading(true)
      const endpoint = filter === 'all' ? '/invoicing/invoices' : `/invoicing/invoices?status=${filter}`
      const data = await api.get<Invoice[]>(endpoint)
      setInvoices(data)
    } catch (error) {
      console.error('Failed to fetch invoices:', error)
    } finally {
      setLoading(false)
    }
  }

  const handleStatusChange = async (invoiceId: string, newStatus: string) => {
    try {
      await api.put(`/invoicing/invoices/${invoiceId}/status`, { status: newStatus })
      fetchInvoices()
    } catch (error) {
      console.error('Failed to update invoice status:', error)
    }
  }

  const getStatusColor = (status: string) => {
    switch (status) {
      case 'draft': return 'bg-gray-100 text-gray-800'
      case 'sent': return 'bg-blue-100 text-blue-800'
      case 'paid': return 'bg-green-100 text-green-800'
      case 'overdue': return 'bg-red-100 text-red-800'
      case 'cancelled': return 'bg-yellow-100 text-yellow-800'
      default: return 'bg-gray-100 text-gray-800'
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
        <h1 className="text-2xl font-bold text-gray-900">Invoicing</h1>
      </div>

      {/* Filter Tabs */}
      <div className="flex gap-2 border-b">
        {['all', 'draft', 'sent', 'paid', 'overdue', 'cancelled'].map((status) => (
          <button
            key={status}
            onClick={() => setFilter(status)}
            className={`px-4 py-2 text-sm font-medium border-b-2 transition ${
              filter === status
                ? 'border-indigo-600 text-indigo-600'
                : 'border-transparent text-gray-500 hover:text-gray-700'
            }`}
          >
            {status.charAt(0).toUpperCase() + status.slice(1)}
          </button>
        ))}
      </div>

      {/* Invoices Table */}
      <div className="bg-white shadow rounded-lg overflow-hidden">
        <table className="min-w-full divide-y divide-gray-200">
          <thead className="bg-gray-50">
            <tr>
              <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">Invoice #</th>
              <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">Customer</th>
              <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">Date</th>
              <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">Due Date</th>
              <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">Total</th>
              <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">Status</th>
              <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase">Actions</th>
            </tr>
          </thead>
          <tbody className="divide-y divide-gray-200">
            {invoices.map((invoice) => (
              <tr key={invoice.id} className="hover:bg-gray-50">
                <td className="px-6 py-4 whitespace-nowrap font-medium">{invoice.invoiceNumber}</td>
                <td className="px-6 py-4 whitespace-nowrap">{invoice.customerName}</td>
                <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-500">{invoice.date}</td>
                <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-500">{invoice.dueDate}</td>
                <td className="px-6 py-4 whitespace-nowrap font-medium">
                  {invoice.currency} {invoice.total.toLocaleString()}
                </td>
                <td className="px-6 py-4 whitespace-nowrap">
                  <span className={`px-2 py-1 text-xs rounded ${getStatusColor(invoice.status)}`}>
                    {invoice.status}
                  </span>
                </td>
                <td className="px-6 py-4 whitespace-nowrap text-sm">
                  <div className="flex gap-2">
                    {invoice.status === 'draft' && (
                      <button
                        onClick={() => handleStatusChange(invoice.id, 'sent')}
                        className="text-indigo-600 hover:text-indigo-800"
                      >
                        Send
                      </button>
                    )}
                    {invoice.status === 'sent' && (
                      <button
                        onClick={() => handleStatusChange(invoice.id, 'paid')}
                        className="text-green-600 hover:text-green-800"
                      >
                        Mark Paid
                      </button>
                    )}
                    <button className="text-gray-600 hover:text-gray-800">View</button>
                  </div>
                </td>
              </tr>
            ))}
          </tbody>
        </table>
        {invoices.length === 0 && (
          <div className="p-8 text-center text-gray-500">No invoices found.</div>
        )}
      </div>
    </div>
  )
}