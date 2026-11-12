{{/* Common labels */}}
{{- define "vimbai.labels" -}}
app.kubernetes.io/name: vimbai
app.kubernetes.io/part-of: vimbai
app.kubernetes.io/managed-by: {{ .Release.Service }}
app.kubernetes.io/instance: {{ .Release.Name }}
{{- end -}}

{{/* Service image */}}
{{- define "vimbai.image" -}}
{{ .Values.global.imageRegistry }}/{{ .Values.global.imageRepository }}/{{ .name }}:latest
{{- end -}}

{{/* Resource defaults */}}
{{- define "vimbai.resources.core" -}}
requests:
  memory: {{ .Values.coreServices.resources.requests.memory }}
  cpu: {{ .Values.coreServices.resources.requests.cpu }}
limits:
  memory: {{ .Values.coreServices.resources.limits.memory }}
  cpu: {{ .Values.coreServices.resources.limits.cpu }}
{{- end -}}

{{/* Resource for core service */}}
{{- define "vimbai.resources.premium" -}}
requests:
  memory: {{ .Values.accountingService.resources.requests.memory }}
  cpu: {{ .Values.accountingService.resources.requests.cpu }}
limits:
  memory: {{ .Values.accountingService.resources.limits.memory }}
  cpu: {{ .Values.accountingService.resources.limits.cpu }}
{{- end -}}
