#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage: db_restore_drill.sh --source-instance <db-instance-id> [--region us-east-1] [--keep-restored]

Runs a live RDS restore drill:
1) Creates a manual snapshot from the source instance
2) Restores a temporary drill instance from that snapshot
3) Waits for availability and reports timing
4) Deletes the restored instance unless --keep-restored is passed
EOF
}

source_instance=""
region="${AWS_REGION:-us-east-1}"
keep_restored="false"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --source-instance)
      source_instance="$2"
      shift 2
      ;;
    --region)
      region="$2"
      shift 2
      ;;
    --keep-restored)
      keep_restored="true"
      shift
      ;;
    *)
      usage
      exit 1
      ;;
  esac
done

if [[ -z "${source_instance}" ]]; then
  usage
  exit 1
fi

timestamp="$(date -u +%Y%m%d%H%M%S)"
snapshot_id="${source_instance}-drill-${timestamp}"
restore_suffix="rdr-${timestamp:8:6}"
restore_prefix="${source_instance:0:50}"
restore_id="${restore_prefix}-${restore_suffix}"

echo "Fetching source instance metadata..."
subnet_group="$(aws rds describe-db-instances \
  --db-instance-identifier "${source_instance}" \
  --region "${region}" \
  --query 'DBInstances[0].DBSubnetGroup.DBSubnetGroupName' \
  --output text)"

sg_ids="$(aws rds describe-db-instances \
  --db-instance-identifier "${source_instance}" \
  --region "${region}" \
  --query 'DBInstances[0].VpcSecurityGroups[].VpcSecurityGroupId' \
  --output text)"

instance_class="$(aws rds describe-db-instances \
  --db-instance-identifier "${source_instance}" \
  --region "${region}" \
  --query 'DBInstances[0].DBInstanceClass' \
  --output text)"

echo "Creating manual snapshot ${snapshot_id}..."
t0="$(date +%s)"
aws rds create-db-snapshot \
  --db-instance-identifier "${source_instance}" \
  --db-snapshot-identifier "${snapshot_id}" \
  --region "${region}" > /dev/null

aws rds wait db-snapshot-completed \
  --db-snapshot-identifier "${snapshot_id}" \
  --region "${region}"
t1="$(date +%s)"

echo "Restoring temporary drill instance ${restore_id}..."
aws rds restore-db-instance-from-db-snapshot \
  --db-instance-identifier "${restore_id}" \
  --db-snapshot-identifier "${snapshot_id}" \
  --db-instance-class "${instance_class}" \
  --db-subnet-group-name "${subnet_group}" \
  --vpc-security-group-ids ${sg_ids} \
  --no-multi-az \
  --no-publicly-accessible \
  --region "${region}" > /dev/null

aws rds wait db-instance-available \
  --db-instance-identifier "${restore_id}" \
  --region "${region}"
t2="$(date +%s)"

snapshot_seconds="$((t1 - t0))"
restore_seconds="$((t2 - t1))"
total_seconds="$((t2 - t0))"

cat <<EOF
Restore drill complete.
  Source instance: ${source_instance}
  Snapshot ID:     ${snapshot_id}
  Restored ID:     ${restore_id}
  Snapshot time:   ${snapshot_seconds}s
  Restore time:    ${restore_seconds}s
  Total RTO test:  ${total_seconds}s
EOF

if [[ "${keep_restored}" == "true" ]]; then
  echo "Keeping restored instance ${restore_id} (requested)."
else
  echo "Deleting restored drill instance ${restore_id}..."
  aws rds delete-db-instance \
    --db-instance-identifier "${restore_id}" \
    --skip-final-snapshot \
    --delete-automated-backups \
    --region "${region}" > /dev/null

  aws rds wait db-instance-deleted \
    --db-instance-identifier "${restore_id}" \
    --region "${region}"
  echo "Deleted restored drill instance."
fi

echo "Manual snapshot retained for audit/verification: ${snapshot_id}"
