import React from 'react';
import { useAuth } from '../contexts/AuthContext';
import { Link } from 'react-router-dom';

const HomePage: React.FC = () => {
  const { logout } = useAuth();

  return (
    <div className="min-h-screen bg-gray-100">
      <nav className="bg-white shadow-sm">
        <div className="max-w-7xl mx-auto px-4 sm:px-6 lg:px-8">
          <div className="flex justify-between h-16">
            <div className="flex">
              <div className="flex-shrink-0 flex items-center">
                <span className="text-xl font-bold text-gray-900">Vimbai Web</span>
              </div>
              <div className="hidden sm:ml-6 sm:flex sm:space-x-8">
                <Link to="/" className="border-indigo-500 text-gray-900 inline-flex items-center px-1 pt-1 border-b-2 text-sm font-medium">
                  Dashboard
                </Link>
                <Link to="/accounts" className="border-transparent text-gray-500 hover:border-gray-300 hover:text-gray-700 inline-flex items-center px-1 pt-1 border-b-2 text-sm font-medium">
                  Accounts
                </Link>
                {/* Add more nav links here */}
              </div>
            </div>
            <div className="hidden sm:ml-6 sm:flex sm:items-center">
              <button
                onClick={logout}
                className="ml-3 inline-flex items-center px-4 py-2 border border-transparent rounded-md shadow-sm text-sm font-medium text-white bg-red-600 hover:bg-red-700 focus:outline-none focus:ring-2 focus:ring-offset-2 focus:ring-red-500"
              >
                Logout
              </button>
            </div>
          </div>
        </div>
      </nav>

      <main className="max-w-7xl mx-auto py-6 sm:px-6 lg:px-8">
        <div className="px-4 py-6 sm:px-0">
          <h1 className="text-3xl font-bold text-gray-900">Welcome to your Vimbai Dashboard!</h1>
          <div className="mt-4 p-6 bg-white shadow-md rounded-lg">
            <p className="text-gray-700">This is your central hub for managing financial data.</p>
            <div className="mt-6 grid grid-cols-1 gap-5 sm:grid-cols-2 lg:grid-cols-3">
              <div className="bg-blue-500 p-5 rounded-lg text-white">
                <h3 className="text-lg font-medium">Total Assets</h3>
                <p className="text-2xl font-bold">$1,234,567.89</p>
              </div>
              <div className="bg-green-500 p-5 rounded-lg text-white">
                <h3 className="text-lg font-medium">Net Income (YTD)</h3>
                <p className="text-2xl font-bold">$123,456.78</p>
              </div>
              <div className="bg-yellow-500 p-5 rounded-lg text-white">
                <h3 className="text-lg font-medium">Cash Flow (Q1)</h3>
                <p className="text-2xl font-bold">$34,567.89</p>
              </div>
            </div>
          </div>
        </div>
      </main>
    </div>
  );
};

export default HomePage;
