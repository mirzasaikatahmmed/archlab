import { useCallback, useEffect, useState } from "react";
import {
  Background,
  Controls,
  ReactFlow,
  addEdge,
  useEdgesState,
  useNodesState,
  type Connection,
  type Edge,
} from "@xyflow/react";
import "@xyflow/react/dist/style.css";
import type { ConfigTemplate, NodeKind, Topology } from "../../bindings";
import { TemplatePicker } from "../TemplatePicker";
import { ArchNodeView } from "./ArchNodeView";
import { NODE_KINDS, createNode, edgeId, fromFlow, toFlow, type ArchNode } from "./flowMapping";

interface Props {
  topology: Topology;
  templates: ConfigTemplate[];
  onChange: (topology: Topology) => void;
}

const nodeTypes = { arch: ArchNodeView };

export function TopologyCanvas({ topology, templates, onChange }: Props) {
  const [initial] = useState(() => toFlow(topology));
  const [nodes, setNodes, onNodesChange] = useNodesState<ArchNode>(initial.nodes);
  const [edges, setEdges, onEdgesChange] = useEdgesState<Edge>(initial.edges);
  const [selectedId, setSelectedId] = useState<string | null>(null);

  useEffect(() => {
    const next = fromFlow(nodes, edges);
    if (JSON.stringify(next) !== JSON.stringify(topology)) {
      onChange(next);
    }
  }, [nodes, edges, topology, onChange]);

  const onConnect = useCallback(
    (c: Connection) => {
      if (c.source === c.target) {
        return;
      }
      setEdges((current) => addEdge({ ...c, id: edgeId(c.source, c.target) }, current));
    },
    [setEdges],
  );

  const addNode = (kind: NodeKind) => {
    setNodes((current) => [...current, createNode(kind, current, crypto.randomUUID())]);
  };

  const selected = nodes.find((n) => n.id === selectedId);
  const updateSelected = (patch: Partial<ArchNode["data"]>) => {
    setNodes((current) =>
      current.map((n) => (n.id === selectedId ? { ...n, data: { ...n.data, ...patch } } : n)),
    );
  };

  return (
    <div className="canvas-layout">
      <div className="palette" role="toolbar" aria-label="Add node">
        {NODE_KINDS.map((k) => (
          <button
            key={k.kind}
            type="button"
            onClick={() => {
              addNode(k.kind);
            }}
          >
            + {k.label}
          </button>
        ))}
      </div>
      <div className="canvas" data-testid="topology-canvas">
        <ReactFlow
          nodes={nodes}
          edges={edges}
          nodeTypes={nodeTypes}
          onNodesChange={onNodesChange}
          onEdgesChange={onEdgesChange}
          onConnect={onConnect}
          onSelectionChange={({ nodes: sel }) => {
            setSelectedId(sel[0]?.id ?? null);
          }}
          deleteKeyCode={["Backspace", "Delete"]}
          fitView
        >
          <Background />
          <Controls />
        </ReactFlow>
      </div>
      <aside className="inspector" aria-label="Node inspector">
        {selected ? (
          <>
            <h3>Node</h3>
            <div className="field">
              <label htmlFor="node-label">Label</label>
              <input
                id="node-label"
                value={selected.data.label}
                maxLength={80}
                onChange={(e) => {
                  updateSelected({ label: e.target.value });
                }}
              />
            </div>
            {selected.data.kind !== "client" && (
              <div className="field">
                <label htmlFor="node-replicas">Replicas</label>
                <input
                  id="node-replicas"
                  type="number"
                  step="1"
                  min="1"
                  max="64"
                  value={Number.isNaN(selected.data.replicas) ? "" : selected.data.replicas}
                  onChange={(e) => {
                    updateSelected({ replicas: e.target.valueAsNumber });
                  }}
                />
                <small>Identical copies behind round-robin; each is a machine.</small>
              </div>
            )}
            <TemplatePicker
              key={selected.id}
              nodeKind={selected.data.kind}
              templates={templates}
              template={selected.data.template}
              settings={selected.data.settings}
              onChange={(template, settings) => {
                updateSelected({ template, settings });
              }}
            />
          </>
        ) : (
          <p className="muted">Select a node to edit it. Drag between handles to connect.</p>
        )}
      </aside>
    </div>
  );
}
