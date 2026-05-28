import { Outlet, NavLink } from 'react-router-dom'
import { useAuth } from '../contexts/AuthContext'

export default function Layout() {
  const { user, logout } = useAuth()

  return (
    <div className="min-h-screen bg-gray-100">
      <nav className="bg-indigo-600 text-white shadow-lg">
        <div className="max-w-7xl mx-auto px-4">
          <div className="flex justify-between h-16">
            <div className="flex items-center">
              <span className="text-xl font-bold">FinAcc</span>
              <div className="ml-10 flex space-x-4">
                <NavLink to="/" className={({isActive}) => isActive ? "bg-indigo-700 px-3 py-2 rounded-md" : "px-3 py-2 rounded-md hover:bg-indigo-500"}>
                  Dashboard
                </NavLink>
                <NavLink to="/accounts" className={({isActive}) => isActive ? "bg-indigo-700 px-3 py-2 rounded-md" : "px-3 py-2 rounded-md hover:bg-indigo-500"}>
                  Accounts
                </NavLink>
                <NavLink to="/journal-entries" className={({isActive}) => isActive ? "bg-indigo-700 px-3 py-2 rounded-md" : "px-3 py-2 rounded-md hover:bg-indigo-500"}>
                  Journal Entries
                </NavLink>
                <NavLink to="/reports" className={({isActive}) => isActive ? "bg-indigo-700 px-3 py-2 rounded-md" : "px-3 py-2 rounded-md hover:bg-indigo-500"}>
                  Reports
                </NavLink>
                <NavLink to="/workflow" className={({isActive}) => isActive ? "bg-indigo-700 px-3 py-2 rounded-md" : "px-3 py-2 rounded-md hover:bg-indigo-500"}>
                  Workflow
                </NavLink>
              </div>
            </div>
            <div className="flex items-center">
              <span className="mr-4">{user?.username}</span>
              <button onClick={logout} className="bg-indigo-700 px-3 py-2 rounded-md hover:bg-indigo-800">
                Logout
              </button>
            </div>
          </div>
        </div>
      </nav>
      <main className="max-w-7xl mx-auto px-4 py-8">
        <Outlet />
      </main>
    </div>
  )
}
