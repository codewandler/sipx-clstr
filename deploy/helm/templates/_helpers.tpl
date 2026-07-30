{{/*
  The one place the chart asks "does this deployment run its own rtpengine?".

  The answer is the configuration document's, not the chart's: a pool with
  `mode: managed` is one the operator deploys and operates itself (KO-7), so the
  workload exists iff `cluster.mediaPool[]` declares such a pool. It is read here
  rather than copied into a `deployment.rtpengine.enabled` beside it, because a
  chart-level boolean is a second spelling of the mode that nothing derives from it
  — free to disagree, and silently — which is what KO-15 removed and what
  sipx-cluster-crd §5's chart-local table now keeps removed.

  `deployment.rtpengine` still carries the workload's *shape* (image, replicas, host
  networking): the document has no field for any of them, so they are not a second
  spelling of anything.

  Emits "true" when any declared pool is operator-managed and the empty string
  otherwise, so the workload guards on it as

      {{- if include "sipx-clstr.mediaPool.managed" . }}

  KO-2 writes that workload; this is the condition it is to use. Nothing renders it
  today, which is exactly why it is defined once instead of being inlined into a
  template that does not exist yet.
*/}}
{{- define "sipx-clstr.mediaPool.managed" -}}
{{- $managed := false -}}
{{- range (.Values.cluster.mediaPool | default list) -}}
{{-   if eq (.mode | default "") "managed" -}}
{{-     $managed = true -}}
{{-   end -}}
{{- end -}}
{{- if $managed }}true{{ end -}}
{{- end -}}
