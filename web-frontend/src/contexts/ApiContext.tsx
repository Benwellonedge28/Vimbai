import React, { createContext, useContext, ReactNode } from 'react'
import axios, { AxiosInstance } from 'axios'

const API_BASE_URL = import.meta.env.VITE_API_BASE_URL || 'http://localhost:8081'

interface ApiContextType {
  client: AxiosInstance
}

const ApiContext = createContext<ApiContextType | undefined>(undefined)

export function ApiProvider({ children }: { children: ReactNode }) {
  const client = axios.create({
    baseURL: API_BASE_URL,
    headers: { 'Content-Type': 'application/json' }
  })

  client.interceptors.request.use((config) => {
    const token = localStorage.getItem('token')
    if (token) {
.config.headers.Authorization = `Bearer ${token}`
    }
    return config
  })

  return (
    <ApiContext.Provider value={{ client }}>
      {children}
    </ApiContext.Provider>
  )
}

export function useApi() {
  const context = useContext(ApiContext)
  if (!context) throw new Error('useApi must be used within ApiProvider')
  return context.client
}
