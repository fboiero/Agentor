import type { ReactNode } from "react";
import { Inbox } from "lucide-react";

export interface Column<T> {
  key: string;
  header: string;
  render: (row: T) => ReactNode;
  align?: "left" | "center" | "right";
}

interface DataTableProps<T> {
  title: string;
  columns: Column<T>[];
  data: T[];
  emptyMessage?: string;
  actions?: ReactNode;
}

export default function DataTable<T>({
  title,
  columns,
  data,
  emptyMessage = "No data found",
  actions,
}: DataTableProps<T>) {
  return (
    <div className="data-table">
      <div className="data-table__header">
        <span className="data-table__title">{title}</span>
        {actions && <div className="data-table__actions">{actions}</div>}
      </div>
      <div className="data-table__scroll">
        <table>
          <thead>
            <tr>
              {columns.map((col) => (
                <th key={col.key} style={{ textAlign: col.align ?? "left" }}>
                  {col.header}
                </th>
              ))}
            </tr>
          </thead>
          <tbody>
            {data.length === 0 ? (
              <tr>
                <td colSpan={columns.length}>
                  <div className="empty-state">
                    <Inbox size={48} strokeWidth={1.2} />
                    <p>{emptyMessage}</p>
                  </div>
                </td>
              </tr>
            ) : (
              data.map((row, idx) => (
                <tr key={idx}>
                  {columns.map((col) => (
                    <td key={col.key} style={{ textAlign: col.align ?? "left" }}>
                      {col.render(row)}
                    </td>
                  ))}
                </tr>
              ))
            )}
          </tbody>
        </table>
      </div>
    </div>
  );
}
