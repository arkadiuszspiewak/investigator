{{- define "investigator-platform.name" -}}
{{- default .Chart.Name .Values.nameOverride | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{- define "investigator-platform.appName" -}}
{{- printf "%s-%s" (include "investigator-platform.fullname" .root) .name | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{- define "investigator-platform.appServiceAccount" -}}
{{- if .app.serviceAccount.create -}}
{{- default (include "investigator-platform.appName" .) .app.serviceAccount.name | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- required (printf "apps.%s.serviceAccount.name is required when create=false" .name) .app.serviceAccount.name | trunc 63 | trimSuffix "-" -}}
{{- end -}}
{{- end -}}

{{- define "investigator-platform.fullname" -}}
{{- if .Values.fullnameOverride -}}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- $name := include "investigator-platform.name" . -}}
{{- if contains $name .Release.Name -}}{{ .Release.Name | trunc 63 | trimSuffix "-" }}
{{- else -}}{{ printf "%s-%s" .Release.Name $name | trunc 63 | trimSuffix "-" }}
{{- end -}}
{{- end -}}
{{- end -}}

{{- define "investigator-platform.labels" -}}
helm.sh/chart: {{ printf "%s-%s" .Chart.Name .Chart.Version | replace "+" "_" }}
app.kubernetes.io/name: {{ include "investigator-platform.name" . }}
app.kubernetes.io/instance: {{ .Release.Name }}
app.kubernetes.io/version: {{ .Chart.AppVersion | quote }}
app.kubernetes.io/managed-by: {{ .Release.Service }}
{{- end -}}

{{- define "investigator-platform.image" -}}
{{- if .image.digest -}}{{ printf "%s@%s" .image.repository .image.digest }}
{{- else -}}{{ printf "%s:%s" .image.repository (default .root.Chart.AppVersion .image.tag) }}
{{- end -}}
{{- end -}}

{{- define "investigator-platform.controllerServiceAccount" -}}
{{- if .Values.investigator.serviceAccount.create -}}
{{- default (printf "%s-controller" (include "investigator-platform.fullname" .)) .Values.investigator.serviceAccount.name | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- required "investigator.serviceAccount.name is required when create=false" .Values.investigator.serviceAccount.name | trunc 63 | trimSuffix "-" -}}
{{- end -}}
{{- end -}}

{{- define "investigator-platform.agentServiceAccount" -}}
{{- if .Values.agent.serviceAccount.create -}}
{{- default (printf "%s-agent" (include "investigator-platform.fullname" .)) .Values.agent.serviceAccount.name | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- required "agent.serviceAccount.name is required when create=false" .Values.agent.serviceAccount.name | trunc 63 | trimSuffix "-" -}}
{{- end -}}
{{- end -}}

{{- define "investigator-platform.serverName" -}}
{{- printf "%s-%s" (include "investigator-platform.fullname" .root) .name | trunc 63 | trimSuffix "-" -}}
{{- end -}}

{{- define "investigator-platform.serverServiceAccount" -}}
{{- if .server.serviceAccount.create -}}
{{- default (include "investigator-platform.serverName" .) .server.serviceAccount.name | trunc 63 | trimSuffix "-" -}}
{{- else -}}
{{- required (printf "mcpServers.%s.serviceAccount.name is required when create=false" .name) .server.serviceAccount.name | trunc 63 | trimSuffix "-" -}}
{{- end -}}
{{- end -}}
