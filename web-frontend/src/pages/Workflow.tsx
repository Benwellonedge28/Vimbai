import { useState, useEffect } from 'react'
import { api } from '../services/api'

export default function Workflow() {
  const [definitions, setDefinitions] = useState<any[]>([])
  const [instances, setInstances] = useState<any[]>([])
  const [loading, setLoading] = useState(true)

  useEffect(() => {
    const fetchData = async () => {
      try {
        const [defs, insts] = await Promise.all([
          api.getWorkflowDefinitions(),
          api.getWorkflowInstances()
        ])
        setDefinitions(defs || [])
        setInstances(insts || [])
      } catch (err) {
        console.error('Failed to load workflow data', err)
      } finally {
        setLoading(false)
      }
    }
    fetchData()
  }, [])

  if (loading) return <div className="text-center py-8">Loading...</div>

  return (
    <div>
      <h1 className="text-3xl font-bold mb-6">Workflow Management</h1>

      <div className="grid grid-cols-1 md:grid-cols-2 gap-6">
        <div className="bg-white p-6 rounded-lg shadow">
          <h2 className="text-xl font-bold mb-4">Workflow Templates</h2>
          {definitions.length === 0 ? (
            <p className="text-gray-500">No workflow templates available</p>
          ) : (
            <ul className="divide-y">
              {definitions.map((def) => (
                <li key={def.id} className="py-3">
                  <p className="font-medium">{def.name}</p>
                  <p className="text-sm text-gray-500">{def.description}</p>
                </li>
              ))}
            </ul>
          )}
        </div>

        <div className="bg-white p-6 rounded-lg shadow">
          <h2 className="text-xl font-bold mb-4">Active Workflows</h2>
          {instances.length === 0 ? (
            <p className="text-gray-500">No active workflows</p>
          ) : (
            <ul className="divide-y">
              {instances.map((inst) => (
                <li key={inst.id} className="py-3 flex justify-between items-center">
                  <div>
                    <p className="font-medium">Instance #{inst.id?.slice(0, 8)}</p>
                    <p className="text-sm text-gray-500">{inst.status}</p>
                  </div>
                  <span className={`px-2 py-1 rounded text-xs ${
                    inst.status === 'approved' ? 'bg-green-100 text-green-800' :
                    inst.status === 'rejected' ? 'bg-red-100 text-red-800' :
                    'bg-yellow-100 text-yellow-800'
                  }`}>{inst.status}</span>
                </li>
              ))}
            </ul>
          )}
        </div>
      </div>
    </div>
  )
}
