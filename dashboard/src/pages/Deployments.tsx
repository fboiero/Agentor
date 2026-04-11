import { useState } from "react";
import { useQuery, useMutation, useQueryClient } from "@tanstack/react-query";
import { Plus, Trash2, ArrowUpDown, X } from "lucide-react";
import DataTable from "../components/DataTable";
import type { Column } from "../components/DataTable";
import StatusBadge from "../components/StatusBadge";
import {
  fetchDeployments,
  createDeployment,
  scaleDeployment,
  deleteDeployment,
} from "../api/client";
import type { Deployment } from "../types";

export default function Deployments() {
  const queryClient = useQueryClient();
  const [showCreate, setShowCreate] = useState(false);
  const [scaleModal, setScaleModal] = useState<{
    id: string;
    name: string;
    replicas: number;
  } | null>(null);

  // Form state
  const [formName, setFormName] = useState("");
  const [formRole, setFormRole] = useState("worker");
  const [formReplicas, setFormReplicas] = useState(1);
  const [scaleCount, setScaleCount] = useState(1);

  const deploymentsQuery = useQuery({
    queryKey: ["deployments"],
    queryFn: fetchDeployments,
  });

  const createMut = useMutation({
    mutationFn: createDeployment,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["deployments"] });
      setShowCreate(false);
      setFormName("");
      setFormReplicas(1);
    },
  });

  const scaleMut = useMutation({
    mutationFn: ({ id, replicas }: { id: string; replicas: number }) =>
      scaleDeployment(id, { replicas }),
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["deployments"] });
      setScaleModal(null);
    },
  });

  const deleteMut = useMutation({
    mutationFn: deleteDeployment,
    onSuccess: () => {
      void queryClient.invalidateQueries({ queryKey: ["deployments"] });
    },
  });

  const columns: Column<Deployment>[] = [
    {
      key: "name",
      header: "Name",
      render: (d) => d.name ?? d.deployment_name ?? "--",
    },
    { key: "role", header: "Role", render: (d) => d.role ?? "--" },
    {
      key: "replicas",
      header: "Replicas",
      align: "center",
      render: (d) => d.replicas ?? "--",
    },
    {
      key: "status",
      header: "Status",
      render: (d) => <StatusBadge status={d.status ?? "unknown"} />,
    },
    {
      key: "tasks",
      header: "Tasks",
      align: "center",
      render: (d) => d.tasks_completed ?? d.total_tasks ?? 0,
    },
    {
      key: "errors",
      header: "Errors",
      align: "center",
      render: (d) => d.errors ?? d.total_errors ?? 0,
    },
    {
      key: "actions",
      header: "Actions",
      render: (d) => (
        <div className="btn-group">
          <button
            className="btn btn--sm"
            onClick={() =>
              setScaleModal({
                id: d.id,
                name: d.name ?? d.deployment_name ?? "",
                replicas: d.replicas ?? 1,
              })
            }
          >
            <ArrowUpDown size={12} /> Scale
          </button>
          <button
            className="btn btn--sm btn--danger"
            onClick={() => {
              if (
                confirm(
                  `Delete deployment "${d.name ?? d.deployment_name}"? This cannot be undone.`,
                )
              )
                deleteMut.mutate(d.id);
            }}
          >
            <Trash2 size={12} /> Delete
          </button>
        </div>
      ),
    },
  ];

  return (
    <section>
      <div className="section-header">
        <div>
          <h1 className="section-title">Deployments</h1>
          <p className="section-subtitle">
            Manage agent deployments and scaling
          </p>
        </div>
      </div>

      {/* Create Form */}
      {showCreate && (
        <div className="form-panel">
          <h3>Create Deployment</h3>
          <div className="form-row">
            <div className="form-group">
              <label htmlFor="deploy-name">Name</label>
              <input
                id="deploy-name"
                type="text"
                placeholder="my-deployment"
                value={formName}
                onChange={(e) => setFormName(e.target.value)}
              />
            </div>
            <div className="form-group">
              <label htmlFor="deploy-role">Role</label>
              <select
                id="deploy-role"
                value={formRole}
                onChange={(e) => setFormRole(e.target.value)}
              >
                <option value="worker">Worker</option>
                <option value="orchestrator">Orchestrator</option>
                <option value="evaluator">Evaluator</option>
              </select>
            </div>
            <div className="form-group" style={{ maxWidth: 120 }}>
              <label htmlFor="deploy-replicas">Replicas</label>
              <input
                id="deploy-replicas"
                type="number"
                min={1}
                max={100}
                value={formReplicas}
                onChange={(e) => setFormReplicas(Number(e.target.value))}
              />
            </div>
            <button
              className="btn btn--primary"
              disabled={createMut.isPending || !formName.trim()}
              onClick={() =>
                createMut.mutate({
                  deployment_name: formName.trim(),
                  role: formRole,
                  replicas: formReplicas,
                })
              }
            >
              {createMut.isPending ? "Creating..." : "Create"}
            </button>
            <button className="btn" onClick={() => setShowCreate(false)}>
              Cancel
            </button>
          </div>
          {createMut.isError && (
            <p className="form-error">{createMut.error.message}</p>
          )}
        </div>
      )}

      <DataTable
        title="Active Deployments"
        columns={columns}
        data={deploymentsQuery.data ?? []}
        emptyMessage="No deployments found"
        actions={
          <button
            className="btn btn--primary btn--sm"
            onClick={() => setShowCreate(!showCreate)}
          >
            <Plus size={14} /> New Deployment
          </button>
        }
      />

      {/* Scale Modal */}
      {scaleModal && (
        <div className="modal-overlay" onClick={() => setScaleModal(null)}>
          <div className="modal" onClick={(e) => e.stopPropagation()}>
            <div className="modal__header">
              <h3>Scale Deployment</h3>
              <button
                className="modal__close"
                onClick={() => setScaleModal(null)}
              >
                <X size={18} />
              </button>
            </div>
            <p className="modal__subtitle">
              Deployment:{" "}
              <strong style={{ fontFamily: "var(--font-mono)" }}>
                {scaleModal.name}
              </strong>
            </p>
            <div className="form-group">
              <label htmlFor="scale-count">New Replica Count</label>
              <input
                id="scale-count"
                type="number"
                min={0}
                max={100}
                value={scaleCount}
                onChange={(e) => setScaleCount(Number(e.target.value))}
              />
            </div>
            <div className="modal__actions">
              <button className="btn" onClick={() => setScaleModal(null)}>
                Cancel
              </button>
              <button
                className="btn btn--primary"
                disabled={scaleMut.isPending}
                onClick={() =>
                  scaleMut.mutate({
                    id: scaleModal.id,
                    replicas: scaleCount,
                  })
                }
              >
                {scaleMut.isPending ? "Scaling..." : "Scale"}
              </button>
            </div>
            {scaleMut.isError && (
              <p className="form-error">{scaleMut.error.message}</p>
            )}
          </div>
        </div>
      )}
    </section>
  );
}
