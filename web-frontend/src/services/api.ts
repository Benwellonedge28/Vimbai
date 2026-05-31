import axios, { AxiosInstance, InternalAxiosRequestConfig } from 'axios'

const API_BASE_URL = import.meta.env.VITE_API_BASE_URL || 'http://localhost:8081'

class ApiService {
  private client: AxiosInstance
  private token: string | null = null

  constructor() {
    this.client = axios.create({
      baseURL: API_BASE_URL,
      headers: { 'Content-Type': 'application/json' }
    })

    this.client.interceptors.request.use((config: InternalAxiosRequestConfig) => {
      if (this.token) {
        config.headers.Authorization = `Bearer ${this.token}`
      }
      return config
    })
  }

  // Generic HTTP methods for flexibility
  async get<T>(url: string): Promise<T> {
    const response = await this.client.get<T>(url)
    return response.data
  }

  async post<T>(url: string, data?: Record<string, unknown>): Promise<T> {
    const response = await this.client.post<T>(url, data)
    return response.data
  }

  async put<T>(url: string, data?: Record<string, unknown>): Promise<T> {
    const response = await this.client.put<T>(url, data)
    return response.data
  }

  async delete<T>(url: string): Promise<T> {
    const response = await this.client.delete<T>(url)
    return response.data
  }

  setToken(token: string) {
    this.token = token
  }

  async login(username: string, password: string) {
    const response = await this.client.post('/identity/login', { username, password })
    return response.data
  }

  async register(username: string, email: string, password: string, role?: string) {
    const response = await this.client.post('/identity/register', { username, email, password, role })
    return response.data
  }

  async getCurrentUser() {
    const response = await this.client.get('/identity/me')
    return response.data
  }

  async getAccounts() {
    const response = await this.client.get('/accounts/')
    return response.data
  }

  async createAccount(account: Record<string, unknown>) {
    const response = await this.client.post('/accounts/', account)
    return response.data
  }

  async getJournalEntries(startDate?: string, endDate?: string) {
    const params = new URLSearchParams()
    if (startDate) params.append('start_date', startDate)
    if (endDate) params.append('end_date', endDate)
    const response = await this.client.get(`/journal-entries/?${params}`)
    return response.data
  }

  async createJournalEntry(entry: Record<string, unknown>) {
    const response = await this.client.post('/journal-entries/', entry)
    return response.data
  }

  async getTrialBalance(asOfDate?: string) {
    const params = asOfDate ? `?as_of_date=${asOfDate}` : ''
    const response = await this.client.get(`/trial-balance/${params}`)
    return response.data
  }

  async getIncomeStatement(startDate: string, endDate: string) {
    const response = await this.client.get(`/income-statement/?start_date=${startDate}&end_date=${endDate}`)
    return response.data
  }

  async getBalanceSheet(asOfDate: string) {
    const response = await this.client.get(`/balance-sheet/?as_of_date=${asOfDate}`)
    return response.data
  }

  async getDashboards() {
    const response = await this.client.get('/reports/dashboards/')
    return response.data
  }

  async executeReport(request: Record<string, unknown>) {
    const response = await this.client.post('/reports/reports/execute', request)
    return response.data
  }

  async getWorkflowDefinitions() {
    const response = await this.client.get('/workflow/workflow-definitions/')
    return response.data
  }

  async createWorkflowInstance(instance: Record<string, unknown>) {
    const response = await this.client.post('/workflow/workflow-instances/', instance)
    return response.data
  }

  async getWorkflowInstances() {
    const response = await this.client.get('/workflow/workflow-instances/')
    return response.data
  }
}

export const api = new ApiService()