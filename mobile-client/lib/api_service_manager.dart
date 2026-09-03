import 'package:vimbai_mobile_client/services/accounting_api_service.dart';
import 'package:vimbai_mobile_client/services/finance_api_service.dart';
import 'package:vimbai_mobile_client/services/multimodal_api_service.dart';

class ApiServiceManager {
  static final ApiServiceManager _instance = ApiServiceManager._internal();

  factory ApiServiceManager() {
    return _instance;
  }

  ApiServiceManager._internal();

  final AccountingApiService accountingApiService = AccountingApiService();
  final FinanceApiService financeApiService = FinanceApiService();
  final MultimodalApiService multimodalApiService = MultimodalApiService();

  // This method could be used to dynamically set the base URL if needed
  void updateBaseUrl(String newUrl) {
    // AppConfig.apiUrl = newUrl; // Would require AppConfig.apiUrl to be mutable
    // For now, it's a const, so we'd restart the app or pass newUrl to services
    // In a real app, inject configurable URLs into services or use a service locator.
  }
}
