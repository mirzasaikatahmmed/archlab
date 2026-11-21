import { Handle, Position, type NodeProps } from "@xyflow/react";
import { kindLabel, type ArchNode } from "./flowMapping";

export function ArchNodeView({ data, selected }: NodeProps<ArchNode>) {
  return (
    <div className={`arch-node kind-${data.kind}${selected ? " selected" : ""}`}>
      <Handle type="target" position={Position.Left} />
      <strong>{data.label}</strong>
      <span>{kindLabel(data.kind)}</span>
      {data.template && <em>{data.template.id}</em>}
      <Handle type="source" position={Position.Right} />
    </div>
  );
}
