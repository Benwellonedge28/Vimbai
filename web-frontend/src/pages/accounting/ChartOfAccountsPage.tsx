import React, { useEffect, useState } from 'react';
import axios from 'axios';
import { useAuth } from '../../context/AuthContext';
import { API_BASE_URL } from '../../config';

interface Account {
  id: string;
  account_number: string;
  account_name: string;
  account_type: string;
  normal_balance: string;
  description?: string;
  parent_account_number?: string;
  created_at: string;
  updated_at: string;
}

const ChartOfAccountsPage: React.FC = () => {
  const { token } = useAuth();
  const [accounts, setAccounts] = useState<Account[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    const fetchAccounts = async () => {
      if (!token) {
        setError('Authentication token missing.');
        setLoading(false);
        return;
      }
      try {
        const response = await axios.get(`${API_BASE_URL}/accounts/`, {
          headers: {
            Authorization: `Bearer ${token}`,
          },
        });
        setAccounts(response.data);
      } catch (err) {
        console.error('Failed to fetch accounts:', err);
        setError('Failed to fetch accounts. Please ensure you have permissions and backend is running.');
      } finally {
        setLoading(false);
      }
    };

    fetchAccounts();
  }, [token]);

  if (loading) {
    return <div className="min-h-screen flex items-center justify-center">Loading Chart of Accounts...</div>;
  }

  if (error) {
    return <div className="min-h-screen flex items-center justify-center text-red-500">{error}</div>;
  }

  return (
    <div className="min-h-screen bg-gray-100 p-8">
      <h1 className="text-3xl font-bold mb-6">Chart of Accounts</h1>
      <div className="bg-white shadow-md rounded-lg p-6">
        {accounts.length === 0 ? (
          <p>No accounts found.</p>
        ) : (
          <div className="overflow-x-auto">
            <table className="min-w-full divide-y divide-gray-200">
              <thead className="bg-gray-50">
                <tr>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">Account Number</th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">Account Name</th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">Type</th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">Normal Balance</th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">Description</th>
                  <th className="px-6 py-3 text-left text-xs font-medium text-gray-500 uppercase tracking-wider">Parent</th>
                </tr>
              </thead>
              <tbody className="bg-white divide-y divide-gray-200">
                {accounts.map((account) => (
                  <tr key={account.id}>
                    <td className="px-6 py-4 whitespace-nowrap text-sm font-medium text-gray-900">{account.account_number}</td>
                    <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-500">{account.account_name}</td>
                    <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-500">{account.account_type}</td>
                    <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-500">{account.normal_balance}</td>
                    <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-500">{account.description || 'N/A'}</td>
                    <td className="px-6 py-4 whitespace-nowrap text-sm text-gray-500">{account.parent_account_number || 'N/A'}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </div>
    </div>
  );
};

export default ChartOfAccountsPage;
