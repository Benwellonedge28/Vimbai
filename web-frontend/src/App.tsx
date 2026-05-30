import { BrowserRouter, Routes, Route, Navigate } from 'react-router-dom'
import { AuthProvider, useAuth } from './contexts/AuthContext'
import { ApiProvider } from './contexts/ApiContext'
import Login from './pages/Login'
import Dashboard from './pages/Dashboard'
import Accounts from './pages/Accounts'
import JournalEntries from './pages/JournalEntries'
import Reports from './pages/Reports'
import Workflow from './pages/Workflow'
import Banking from './pages/Banking'
import Invoicing from './pages/Invoicing'
import Layout from './components/Layout'

function ProtectedRoute({ children }: { children: React.ReactNode }) {
  const { isAuthenticated } = useAuth()
  return isAuthenticated ? <>{children}</> : <Navigate to="/login" />
}

function AppRoutes() {
  return (
    <Routes>
      <Route path="/login" element={<Login />} />
      <Route path="/" element={
        <ProtectedRoute>
          <Layout />
        </ProtectedRoute>
      }>
        <Route index element={<Dashboard />} />
        <Route path="accounts" element={<Accounts />} />
        <Route path="journal-entries" element={<JournalEntries />} />
        <Route path="reports" element={<Reports />} />
        <Route path="workflow" element={<Workflow />} />
        <Route path="banking" element={<Banking />} />
        <Route path="invoicing" element={<Invoicing />} />
      </Route>
    </Routes>
  )
}

export default function App() {
  return (
    <BrowserRouter>
      <AuthProvider>
        <ApiProvider>
          <AppRoutes />
        </ApiProvider>
      </AuthProvider>
    </BrowserRouter>
  )
}
